use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, Ident, ItemEnum, TypePath, Variant};

use crate::common::{SCHEMA_PARAM, parse_kind_attr};
use crate::enum_derives::{parse_comma_separated_types, require_attribute, require_type_param};

const NODE_KIND_ATTR: &str = "node_kind";
const PROPERTY_KIND_PARAM: &str = "property_kind";
const PROPERTIES_ATTR: &str = "properties";

pub struct NodeKindConfig {
    pub schema: TypePath,
    pub property_kind: TypePath,
}

pub fn parse_node_kind_config(input: &ItemEnum) -> Result<NodeKindConfig, Error> {
    let (attr, args) = parse_kind_attr(
        NODE_KIND_ATTR,
        input,
        &format!("{SCHEMA_PARAM} = ..., {PROPERTY_KIND_PARAM} = ..."),
    )?;

    Ok(NodeKindConfig {
        schema: require_type_param(attr, &args, SCHEMA_PARAM, NODE_KIND_ATTR)?,
        property_kind: require_type_param(attr, &args, PROPERTY_KIND_PARAM, NODE_KIND_ATTR)?,
    })
}

fn node_struct_name(name: &Ident) -> Ident {
    format_ident!("{}Node", name)
}

fn node_builder_name(name: &Ident) -> Ident {
    format_ident!("{}Builder", name)
}

fn expand_node_structs(
    name: &Ident,
    vis: &syn::Visibility,
    vars: &[(&Ident, Ident)],
    config: &NodeKindConfig,
) -> TokenStream {
    let schema_ty = &config.schema;
    let structs = vars.iter().map(|(v, struct_name)| {
        quote! {
            #vis struct #struct_name<'a>(&'a ::flatpg::graph::Graph<#schema_ty>, usize);

            impl<'a> #struct_name<'a>
            {
                #vis fn new(graph: &'a ::flatpg::graph::Graph<#schema_ty>, seq: usize) -> Self {
                    Self(graph, seq)
                }
            }

            impl<'a> ::flatpg::node::StoredNode<#schema_ty> for #struct_name<'a> {
                #[inline]
                fn graph(&self) -> &::flatpg::graph::Graph<#schema_ty> {
                    self.0
                }
                #[inline]
                fn seq(&self) -> usize {
                    self.1
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

pub fn node_structs_derive(
    input: &ItemEnum,
    config: &NodeKindConfig,
) -> Result<TokenStream, Error> {
    let ident = &input.ident;
    let vis = &input.vis;
    let schema_ty = &config.schema;
    let property_kind_ty = &config.property_kind;

    let variants = input
        .variants
        .iter()
        .map(|Variant { ident: variant, .. }| (variant, node_struct_name(variant)))
        .collect::<Vec<_>>();

    let structs = expand_node_structs(ident, vis, &variants, config);

    let builders_def = variants.iter().map(|(v, struct_name)| {
        let builder_name = node_builder_name(struct_name);
        quote! {
            pub struct #builder_name(::flatpg::node::NewNode<#schema_ty>);

            impl #builder_name {
                pub fn new() -> Self {
                    Self(::flatpg::node::NewNode::new(#ident::#v))
                }

                pub fn add_property<T: ::core::convert::Into<::flatpg::property::PropertyValue>>(
                    mut self,
                    prop_kind: #property_kind_ty,
                    value: T,
                ) -> ::core::result::Result<Self, ::flatpg::error::Error> {
                    self.0.add_property(prop_kind, value)?;
                    ::core::result::Result::Ok(self)
                }

                pub fn build(self) -> ::flatpg::node::NewNode<#schema_ty> {
                    self.0
                }
            }
        }
    });

    let node_variants = variants.iter().map(|(v, struct_name)| {
        quote! {
            #v(#struct_name<'a>)
        }
    });

    let node_new_variants = variants.iter().map(|(v, struct_name)| {
        quote! {
            #ident::#v => Self::#v(#struct_name::new(graph, seq))
        }
    });

    let match_node = variants
        .iter()
        .map(|(v, _)| {
            quote! {
                Node::#v(node)
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        #vis mod builders {
            use super::*;
            #(
                #builders_def
            )*
        }

        #structs

        #vis enum Node<'a> {
            #(
                #node_variants,
            )*
        }

        impl<'a> Node<'a> {
            #vis fn new(
                graph: &'a ::flatpg::graph::Graph<#schema_ty>,
                kind: #ident,
                seq: usize,
            ) -> Self {
                match kind {
                    #(
                        #node_new_variants,
                    )*
                }
            }
        }

        impl<'a> ::flatpg::node::StoredNode<#schema_ty> for Node<'a> {

            fn graph(&self) -> &::flatpg::graph::Graph<#schema_ty> {
                match self {
                    #(
                        #match_node => node.graph(),
                    )*
                }
            }
            fn seq(&self) -> usize {
                match self {
                    #(
                        #match_node => node.seq(),
                    )*
                }
            }
            fn kind(&self) -> #ident {
                match self {
                    #(
                        #match_node => node.kind(),
                    )*
                }
            }
        }
    })
}

pub fn node_available_properties_derive(
    input: &ItemEnum,
    config: &NodeKindConfig,
) -> Result<TokenStream, Error> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let property_kind_ty = &config.property_kind;
    let schema_ty = &config.schema;

    let variants_with_args = input
        .variants
        .iter()
        .map(|Variant { ident, attrs, .. }| {
            let props = require_attribute(PROPERTIES_ATTR, attrs, ident, "variant", "...")
                .and_then(parse_comma_separated_types)?
                .into_iter()
                .collect::<Vec<_>>();
            Ok((ident, props))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let props_match_arms = variants_with_args.iter().map(|(variant, props)| {
        quote! {#name::#variant => &[#(#property_kind_ty::#props,)*]}
    });

    let prop_traits_impls = variants_with_args.iter().flat_map(|(variant, props)| {
        let struct_name = node_struct_name(variant);
        props.iter().map(move |p| {
            quote! {impl<'a> #p<#schema_ty> for #struct_name<'a> {}}
        })
    });

    Ok(quote! {
        impl #impl_generics ::flatpg::prelude::AvailableProperties<#property_kind_ty>
            for #name #ty_generics #where_clause
        {
            fn properties(&self) -> &'static [#property_kind_ty] {
                match self {
                    #(#props_match_arms,)*
                }
            }
        }
        #(#prop_traits_impls)*
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{find_impl, find_method, parse_enum, parse_output};
    use syn::{File, Item, ItemImpl, ItemMod, ItemStruct, Visibility, parse_str};

    fn config(schema: &str) -> NodeKindConfig {
        NodeKindConfig {
            schema: parse_str(schema).expect("failed to parse schema type"),
            property_kind: parse_str("Property").expect("failed to parse property_kind type"),
        }
    }

    #[test]
    fn available_properties_impl_forwards_generics_from_input_enum() {
        let input = parse_enum(
            r#"enum SimpleNode<const N: usize> {
                #[properties()] A,
            }"#,
        );
        let file = parse_output(
            node_available_properties_derive(&input, &config("SimpleSchema")).unwrap(),
        );

        let impl_block = find_impl(&file, "AvailableProperties", "SimpleNode")
            .expect("impl AvailableProperties for SimpleNode not found");
        assert_eq!(impl_block.generics.params.len(), 1);
    }

    fn find_struct_in<'a>(items: &'a [Item], name: &str) -> Option<&'a ItemStruct> {
        items.iter().find_map(|item| {
            let Item::Struct(s) = item else { return None };
            (s.ident == name).then_some(s)
        })
    }

    fn find_enum_in<'a>(items: &'a [Item], name: &str) -> Option<&'a ItemEnum> {
        items.iter().find_map(|item| {
            let Item::Enum(e) = item else { return None };
            (e.ident == name).then_some(e)
        })
    }

    fn find_mod_in<'a>(items: &'a [Item], name: &str) -> Option<&'a ItemMod> {
        items.iter().find_map(|item| {
            let Item::Mod(m) = item else { return None };
            (m.ident == name).then_some(m)
        })
    }

    fn find_inherent_impl_in<'a>(items: &'a [Item], self_type: &str) -> Option<&'a ItemImpl> {
        items.iter().find_map(|item| {
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

    fn find_struct<'a>(file: &'a File, name: &str) -> Option<&'a ItemStruct> {
        find_struct_in(&file.items, name)
    }

    fn find_enum<'a>(file: &'a File, name: &str) -> Option<&'a ItemEnum> {
        find_enum_in(&file.items, name)
    }

    fn find_mod<'a>(file: &'a File, name: &str) -> Option<&'a ItemMod> {
        find_mod_in(&file.items, name)
    }

    fn find_inherent_impl<'a>(file: &'a File, self_type: &str) -> Option<&'a ItemImpl> {
        find_inherent_impl_in(&file.items, self_type)
    }

    fn mod_items(m: &ItemMod) -> &[Item] {
        &m.content.as_ref().expect("module has inline content").1
    }

    /// `#vis` is spliced onto every generated struct/enum/mod/inherent-method as a freestanding
    /// item visibility (not a trait impl method, so unlike `ItemAll` etc. there's no E0449 risk —
    /// but the generated items must still track the source enum's visibility rather than always
    /// being `pub`, or a private node-kind enum's supposedly-internal types would leak wider than
    /// the author intended). The `ANodeBuilder` struct/methods nested inside `mod builders` are
    /// the one exception: they stay `pub` unconditionally, gated by `mod builders`'s own `#vis`
    /// instead — see the comment on `builders_def` in `node_structs_derive`.
    fn assert_vis_matches(actual: &Visibility, expect_pub: bool, what: &str) {
        let is_pub = matches!(actual, Visibility::Public(_));
        assert_eq!(
            is_pub,
            expect_pub,
            "expected `{what}` to be {} (matching the source enum's visibility) but it was {}",
            if expect_pub { "pub" } else { "non-pub" },
            if is_pub { "pub" } else { "non-pub" }
        );
    }

    fn check_node_structs_visibility(enum_src: &str, expect_pub: bool) {
        let input = parse_enum(enum_src);
        let cfg = config("MySchema");
        let file = parse_output(node_structs_derive(&input, &cfg).unwrap());

        let node_struct = find_struct(&file, "ANode").expect("ANode struct not found");
        assert_vis_matches(&node_struct.vis, expect_pub, "struct ANode");

        let node_impl =
            find_inherent_impl(&file, "ANode").expect("inherent impl for ANode not found");
        let new_method = find_method(node_impl, "new").expect("ANode::new not found");
        assert_vis_matches(&new_method.vis, expect_pub, "fn ANode::new");

        let builders_mod = find_mod(&file, "builders").expect("builders module not found");
        assert_vis_matches(&builders_mod.vis, expect_pub, "mod builders");

        // `ANodeBuilder` lives inside `mod builders`, which is already the real privacy
        // boundary asserted above — it must stay unconditionally `pub` regardless of the
        // source enum's visibility, or it becomes unreachable even from beside the enum
        // (private always means "private to the immediate enclosing module", which here
        // would be `builders`, not the enum's own module).
        let inner = mod_items(builders_mod);
        let builder_struct =
            find_struct_in(inner, "ANodeBuilder").expect("ANodeBuilder struct not found");
        assert_vis_matches(&builder_struct.vis, true, "struct ANodeBuilder");

        let builder_impl = find_inherent_impl_in(inner, "ANodeBuilder")
            .expect("inherent impl for ANodeBuilder not found");
        for method_name in ["new", "add_property", "build"] {
            let method = find_method(builder_impl, method_name)
                .unwrap_or_else(|| panic!("ANodeBuilder::{method_name} not found"));
            assert_vis_matches(
                &method.vis,
                true,
                &format!("fn ANodeBuilder::{method_name}"),
            );
        }

        let node_enum = find_enum(&file, "Node").expect("Node enum not found");
        assert_vis_matches(&node_enum.vis, expect_pub, "enum Node");

        let node_enum_impl =
            find_inherent_impl(&file, "Node").expect("inherent impl for Node not found");
        let node_new = find_method(node_enum_impl, "new").expect("Node::new not found");
        assert_vis_matches(&node_new.vis, expect_pub, "fn Node::new");
    }

    #[test]
    fn node_structs_derive_pub_enum_generates_pub_items() {
        check_node_structs_visibility("pub enum N { A, B }", true);
    }

    #[test]
    fn node_structs_derive_private_enum_does_not_leak_pub_items() {
        check_node_structs_visibility("enum N { A, B }", false);
    }
}
