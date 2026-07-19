use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Error, Ident, ItemEnum, TypePath, Variant, parse2, punctuated::Punctuated};

use crate::enum_derives::find_attribute;

const EDGE_KIND_ATTR: &str = "edge_kind";
const SCHEMA_PARAM: &str = "schema";

pub struct EdgeKindConfig {
    pub schema: TypePath,
}

pub fn parse_edge_kind_config(input: &ItemEnum) -> Result<EdgeKindConfig, Error> {
    let attr = find_attribute(EDGE_KIND_ATTR, &input.attrs).ok_or_else(|| {
        Error::new_spanned(
            &input.ident,
            format!("enum must be annotated with #[{EDGE_KIND_ATTR}({SCHEMA_PARAM} = ...)]"),
        )
    })?;

    let args =
        attr.parse_args_with(Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated)?;

    let get = |key: &str| -> Result<TypePath, Error> {
        args.iter()
            .find(|m| m.path.is_ident(key))
            .ok_or_else(|| {
                Error::new_spanned(
                    attr,
                    format!("missing `{key}` parameter in #[{EDGE_KIND_ATTR}(...)]"),
                )
            })
            .and_then(|m| parse2(m.value.to_token_stream()))
    };

    Ok(EdgeKindConfig {
        schema: get(SCHEMA_PARAM)?,
    })
}

fn edge_struct_name(name: &Ident) -> Ident {
    format_ident!("{}Edge", name)
}

fn expand_edge_structs(
    name: &Ident,
    vars: &[(&Ident, Ident)],
    config: &EdgeKindConfig,
) -> TokenStream {
    let schema_ty = &config.schema;
    let structs = vars.iter().map(|(v, struct_name)| {
        quote! {
            pub struct #struct_name<'a> {
                graph: &'a flatpg::graph::Graph<#schema_ty>,
                src_node: flatpg::node::Node<#schema_ty>,
                dst_node: flatpg::node::Node<#schema_ty>,
                direction: flatpg::edge::Direction,
                seq: usize,
            }

            impl<'a> #struct_name<'a> {
                pub fn new(
                    graph: &'a flatpg::graph::Graph<#schema_ty>,
                    src_node: flatpg::node::Node<#schema_ty>,
                    dst_node: flatpg::node::Node<#schema_ty>,
                    direction: flatpg::edge::Direction,
                    seq: usize,
                ) -> Self {
                    Self {
                        graph,
                        src_node,
                        dst_node,
                        direction,
                        seq,
                    }
                }
            }

            impl<'a> flatpg::edge::StoredEdge<#schema_ty> for #struct_name<'a> {
                #[inline]
                fn graph(&self) -> &flatpg::graph::Graph<#schema_ty> {
                    self.graph
                }
                #[inline]
                fn src_node(&self) -> flatpg::node::Node<#schema_ty> {
                    self.src_node
                }
                #[inline]
                fn dst_node(&self) -> flatpg::node::Node<#schema_ty> {
                    self.dst_node
                }
                #[inline]
                fn direction(&self) -> flatpg::edge::Direction {
                    self.direction
                }
                #[inline]
                fn seq(&self) -> usize {
                    self.seq
                }
                #[inline]
                fn kind(&self) -> #name {
                    #name::#v
                }
            }
        }
    });
    quote! {
        #(
            #structs
        )*
    }
}

pub fn edge_structs_derive(
    input: &ItemEnum,
    config: &EdgeKindConfig,
) -> Result<TokenStream, Error> {
    let ident = &input.ident;
    let schema_ty = &config.schema;

    let variants = input
        .variants
        .iter()
        .map(|Variant { ident: variant, .. }| (variant, edge_struct_name(variant)))
        .collect::<Vec<_>>();

    let structs = expand_edge_structs(ident, &variants, config);

    let gedge_variants = variants.iter().map(|(v, struct_name)| {
        quote! {
            #v(#struct_name<'a>)
        }
    });

    let gedge_new_variants = variants.iter().map(|(v, struct_name)| {
        quote! {
            #ident::#v => Self::#v(#struct_name::new(graph, src_node, dst_node, direction, seq))
        }
    });

    let match_gedge = variants
        .iter()
        .map(|(v, _)| {
            quote! {
                GEdge::#v(edge)
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        #structs

        pub enum GEdge<'a> {
            #(
                #gedge_variants,
            )*
        }

        impl<'a> GEdge<'a> {
            pub fn new(
                graph: &'a flatpg::graph::Graph<#schema_ty>,
                kind: #ident,
                src_node: flatpg::node::Node<#schema_ty>,
                dst_node: flatpg::node::Node<#schema_ty>,
                direction: flatpg::edge::Direction,
                seq: usize,
            ) -> Self {
                match kind {
                    #(
                        #gedge_new_variants,
                    )*
                }
            }
        }

        impl<'a> flatpg::edge::StoredEdge<#schema_ty> for GEdge<'a> {
            fn graph(&self) -> &flatpg::graph::Graph<#schema_ty> {
                match self {
                    #(
                        #match_gedge => edge.graph(),
                    )*
                }
            }
            fn src_node(&self) -> flatpg::node::Node<#schema_ty> {
                match self {
                    #(
                        #match_gedge => edge.src_node(),
                    )*
                }
            }
            fn dst_node(&self) -> flatpg::node::Node<#schema_ty> {
                match self {
                    #(
                        #match_gedge => edge.dst_node(),
                    )*
                }
            }
            fn direction(&self) -> flatpg::edge::Direction {
                match self {
                    #(
                        #match_gedge => edge.direction(),
                    )*
                }
            }
            fn seq(&self) -> usize {
                match self {
                    #(
                        #match_gedge => edge.seq(),
                    )*
                }
            }
            fn kind(&self) -> #ident {
                match self {
                    #(
                        #match_gedge => edge.kind(),
                    )*
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Expr, File, ImplItem, Item, Stmt, parse_str};

    fn parse_enum(src: &str) -> ItemEnum {
        parse_str(src).expect("failed to parse enum")
    }

    fn parse_output(ts: TokenStream) -> File {
        parse2(ts).expect("generated output is not valid Rust")
    }

    fn find_impl<'a>(
        file: &'a File,
        trait_name: &str,
        self_type: &str,
    ) -> Option<&'a syn::ItemImpl> {
        file.items.iter().find_map(|item| {
            let Item::Impl(impl_block) = item else {
                return None;
            };
            let trait_matches = impl_block.trait_.as_ref().is_some_and(|(_, path, _)| {
                path.segments.last().is_some_and(|s| s.ident == trait_name)
            });
            let type_matches = match impl_block.self_ty.as_ref() {
                syn::Type::Path(tp) => tp
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == self_type),
                _ => false,
            };
            (trait_matches && type_matches).then_some(impl_block)
        })
    }

    fn find_inherent_impl<'a>(file: &'a File, self_type: &str) -> Option<&'a syn::ItemImpl> {
        file.items.iter().find_map(|item| {
            let Item::Impl(impl_block) = item else {
                return None;
            };
            let type_matches = match impl_block.self_ty.as_ref() {
                syn::Type::Path(tp) => tp
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == self_type),
                _ => false,
            };
            (impl_block.trait_.is_none() && type_matches).then_some(impl_block)
        })
    }

    fn find_method<'a>(impl_block: &'a syn::ItemImpl, name: &str) -> Option<&'a syn::ImplItemFn> {
        impl_block.items.iter().find_map(|item| {
            let ImplItem::Fn(method) = item else {
                return None;
            };
            (method.sig.ident == name).then_some(method)
        })
    }

    fn find_struct<'a>(file: &'a File, name: &str) -> Option<&'a syn::ItemStruct> {
        file.items.iter().find_map(|item| {
            let Item::Struct(s) = item else {
                return None;
            };
            (s.ident == name).then_some(s)
        })
    }

    fn find_enum<'a>(file: &'a File, name: &str) -> Option<&'a syn::ItemEnum> {
        file.items.iter().find_map(|item| {
            let Item::Enum(e) = item else {
                return None;
            };
            (e.ident == name).then_some(e)
        })
    }

    fn match_arm_count(method: &syn::ImplItemFn) -> Option<usize> {
        method.block.stmts.iter().find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Match(m), _) => Some(m.arms.len()),
            _ => None,
        })
    }

    fn edge_kind_config(schema: &str) -> EdgeKindConfig {
        parse_edge_kind_config(&parse_enum(&format!(
            "#[edge_kind(schema = {schema})] enum E {{ A, B }}"
        )))
        .expect("valid config")
    }

    #[test]
    fn parse_edge_kind_config_missing_attribute_errors() {
        let input = parse_enum("enum E { A, B }");
        assert!(parse_edge_kind_config(&input).is_err());
    }

    #[test]
    fn parse_edge_kind_config_missing_schema_param_errors() {
        let input = parse_enum("#[edge_kind()] enum E { A, B }");
        assert!(parse_edge_kind_config(&input).is_err());
    }

    #[test]
    fn parse_edge_kind_config_valid_parses_schema() {
        let config = edge_kind_config("MySchema");
        assert!(config.schema.path.is_ident("MySchema"));
    }

    #[test]
    fn edge_structs_derive_generates_variant_struct_with_new_and_stored_edge_impl() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] A,
                #[property(typ = String)] B,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let s = find_struct(&file, "AEdge").expect("AEdge struct not found");
        assert_eq!(s.fields.iter().count(), 5);

        let inherent =
            find_inherent_impl(&file, "AEdge").expect("inherent impl for AEdge not found");
        assert!(find_method(inherent, "new").is_some());

        let stored_edge =
            find_impl(&file, "StoredEdge", "AEdge").expect("StoredEdge impl for AEdge not found");
        for method in ["graph", "src_node", "dst_node", "direction", "seq", "kind"] {
            assert!(
                find_method(stored_edge, method).is_some(),
                "missing method {method}"
            );
        }
    }

    #[test]
    fn edge_structs_derive_generates_gedge_enum_with_variant_per_input() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] A,
                #[property(typ = String)] B,
                #[property(typ = Int)] C,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let gedge = find_enum(&file, "GEdge").expect("GEdge enum not found");
        assert_eq!(gedge.variants.len(), 3);
    }

    #[test]
    fn edge_structs_derive_gedge_new_and_stored_edge_impl_match_arm_counts() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] A,
                #[property(typ = String)] B,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let gedge_impl =
            find_inherent_impl(&file, "GEdge").expect("inherent impl for GEdge not found");
        let new_method = find_method(gedge_impl, "new").expect("GEdge::new not found");
        assert_eq!(match_arm_count(new_method), Some(2));

        let stored_edge =
            find_impl(&file, "StoredEdge", "GEdge").expect("StoredEdge impl for GEdge not found");
        for method in ["graph", "src_node", "dst_node", "direction", "seq", "kind"] {
            let m = find_method(stored_edge, method)
                .unwrap_or_else(|| panic!("missing method {method}"));
            assert_eq!(
                match_arm_count(m),
                Some(2),
                "wrong match arm count for {method}"
            );
        }
    }
}
