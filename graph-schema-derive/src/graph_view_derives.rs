use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, Ident, ItemEnum, Variant, punctuated::Punctuated, token::Comma};

use crate::common::method_ident;
use crate::edge_structs_derives::EdgeKindConfig;
use crate::node_structs_derives::NodeKindConfig;

fn duplicate_method_name_error(dup: &Ident, name: &str) -> Error {
    Error::new_spanned(
        dup,
        format!(
            "duplicate node-kind accessor method `{name}` — two variants snake_case to the same \
             method name, which would make `.{name}()` ambiguous once both generated accessor \
             traits are blanket-implemented for the same type; rename one of the variants"
        ),
    )
}

fn check_no_method_name_collisions(variants: &Punctuated<Variant, Comma>) -> Result<(), Error> {
    let mut seen: Vec<String> = Vec::new();
    for variant in variants {
        let name = method_ident(&variant.ident).to_string();
        if seen.contains(&name) {
            return Err(duplicate_method_name_error(&variant.ident, &name));
        }
        seen.push(name);
    }
    Ok(())
}

pub fn node_accessor_traits_derive(
    input: &ItemEnum,
    config: &NodeKindConfig,
) -> Result<TokenStream, Error> {
    check_no_method_name_collisions(&input.variants)?;

    let enum_ident = &input.ident;
    let vis = &input.vis;
    let schema_ty = &config.schema;

    let items = input.variants.iter().map(|variant| {
        let v = &variant.ident;
        let trait_name = format_ident!("{}NodesAccessor", v);
        let method_name = method_ident(v);
        quote! {
            #vis trait #trait_name: flatpg::graph::GraphView<#schema_ty> {
                fn #method_name(&self) -> impl Iterator<Item = Node<'_>> + '_ {
                    self.graph()
                        .nodes_by_kind(#enum_ident::#v)
                        .map(|node| Node::new(self.graph(), node.kind(), node.seq()))
                }
            }

            impl<T: flatpg::graph::GraphView<#schema_ty>> #trait_name for T {}
        }
    });

    Ok(quote! { #(#items)* })
}

pub fn edges_accessor_trait_derive(input: &ItemEnum, config: &EdgeKindConfig) -> TokenStream {
    let enum_ident = &input.ident;
    let vis = &input.vis;
    let schema_ty = &config.schema;

    quote! {
        #vis trait EdgesAccessor: flatpg::graph::GraphView<#schema_ty> {
            fn edges<'a>(
                &'a self,
                src_node: flatpg::node::NodeId<#schema_ty>,
                edge_kind: #enum_ident,
                direction: flatpg::edge::Direction,
            ) -> Result<impl ExactSizeIterator<Item = Edge<'a>> + 'a, flatpg::error::Error> {
                let graph = self.graph();
                Ok(graph
                    .get_edges(src_node, edge_kind, direction)?
                    .map(move |e| {
                        Edge::new(
                            graph,
                            e.kind(),
                            e.src_node(),
                            e.dst_node(),
                            e.direction(),
                            e.seq(),
                        )
                    }))
            }
        }

        impl<T: flatpg::graph::GraphView<#schema_ty>> EdgesAccessor for T {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{find_impl, has_compile_error, parse_enum, parse_output};
    use syn::{ItemTrait, TraitItemFn, parse_str};

    fn config(schema: &str) -> NodeKindConfig {
        NodeKindConfig {
            schema: parse_str(schema).expect("failed to parse schema type"),
            property_kind: parse_str("Property").expect("failed to parse property_kind type"),
        }
    }

    fn edge_config(schema: &str) -> EdgeKindConfig {
        EdgeKindConfig {
            schema: parse_str(schema).expect("failed to parse schema type"),
        }
    }

    fn find_trait<'a>(file: &'a syn::File, name: &str) -> Option<&'a ItemTrait> {
        file.items.iter().find_map(|item| {
            let syn::Item::Trait(t) = item else {
                return None;
            };
            (t.ident == name).then_some(t)
        })
    }

    fn find_trait_method<'a>(t: &'a ItemTrait, name: &str) -> Option<&'a TraitItemFn> {
        t.items.iter().find_map(|item| {
            let syn::TraitItem::Fn(f) = item else {
                return None;
            };
            (f.sig.ident == name).then_some(f)
        })
    }

    fn has_supertrait(t: &ItemTrait, name: &str) -> bool {
        t.supertraits.iter().any(|bound| {
            let syn::TypeParamBound::Trait(tb) = bound else {
                return false;
            };
            tb.path.segments.last().is_some_and(|s| s.ident == name)
        })
    }

    #[test]
    fn generates_one_trait_and_blanket_impl_per_variant() {
        let input = parse_enum("enum SimpleNode { A, B }");
        let file =
            parse_output(node_accessor_traits_derive(&input, &config("SimpleSchema")).unwrap());

        let a_trait = find_trait(&file, "ANodesAccessor").expect("ANodesAccessor trait not found");
        assert!(find_trait_method(a_trait, "a").is_some());
        assert!(has_supertrait(a_trait, "GraphView"));
        assert!(find_impl(&file, "ANodesAccessor", "T").is_some());

        let b_trait = find_trait(&file, "BNodesAccessor").expect("BNodesAccessor trait not found");
        assert!(find_trait_method(b_trait, "b").is_some());
        assert!(find_impl(&file, "BNodesAccessor", "T").is_some());
    }

    #[test]
    fn escapes_keyword_collision_in_method_name() {
        let input = parse_enum("enum SimpleNode { Ref }");
        let file =
            parse_output(node_accessor_traits_derive(&input, &config("SimpleSchema")).unwrap());

        let t = find_trait(&file, "RefNodesAccessor").expect("RefNodesAccessor trait not found");
        assert_eq!(t.items.len(), 1);
        let syn::TraitItem::Fn(f) = &t.items[0] else {
            panic!("expected fn")
        };
        use syn::ext::IdentExt;
        assert_eq!(f.sig.ident.unraw().to_string(), "ref");
    }

    #[test]
    fn duplicate_method_names_emit_compile_error() {
        let input = parse_enum("enum SimpleNode { UserId, UserID }");
        assert!(has_compile_error(
            node_accessor_traits_derive(&input, &config("SimpleSchema"))
                .unwrap_or_else(Error::into_compile_error)
        ));
    }

    #[test]
    fn generates_edges_accessor_trait_and_blanket_impl() {
        let input = parse_enum("enum SimpleEdge { Base, Extended }");
        let file = parse_output(edges_accessor_trait_derive(
            &input,
            &edge_config("SimpleSchema"),
        ));

        let t = find_trait(&file, "EdgesAccessor").expect("EdgesAccessor trait not found");
        assert!(find_trait_method(t, "edges").is_some());
        assert!(has_supertrait(t, "GraphView"));
        assert!(find_impl(&file, "EdgesAccessor", "T").is_some());
    }
}
