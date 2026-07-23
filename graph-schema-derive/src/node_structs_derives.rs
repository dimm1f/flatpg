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
    vars: &[(&Ident, Ident)],
    config: &NodeKindConfig,
) -> TokenStream {
    let schema_ty = &config.schema;
    let structs = vars.iter().map(|(v, struct_name)| {
        quote! {
            pub struct #struct_name<'a>(&'a flatpg::graph::Graph<#schema_ty>, usize);

            impl<'a> #struct_name<'a>
            {
                pub fn new(graph: &'a flatpg::graph::Graph<#schema_ty>, seq: usize) -> Self {
                    Self(graph, seq)
                }
            }

            impl<'a> flatpg::node::StoredNode<#schema_ty> for #struct_name<'a> {
                #[inline]
                fn graph(&self) -> &flatpg::graph::Graph<#schema_ty> {
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
    let schema_ty = &config.schema;
    let property_kind_ty = &config.property_kind;

    let variants = input
        .variants
        .iter()
        .map(|Variant { ident: variant, .. }| (variant, node_struct_name(variant)))
        .collect::<Vec<_>>();

    let structs = expand_node_structs(ident, &variants, config);

    let builders_def = variants.iter().map(|(v, struct_name)| {
        let builder_name = node_builder_name(struct_name);
        quote! {
            pub struct #builder_name(flatpg::node::NewNode<#schema_ty>);

            impl #builder_name {
                pub fn new() -> Self {
                    Self(flatpg::node::NewNode::new(#ident::#v))
                }

                pub fn add_property<T: Into<flatpg::property::PropertyValue>>(mut self, prop_kind: #property_kind_ty, value: T) -> Result<Self, flatpg::error::Error> {
                    self.0.add_property(prop_kind, value)?;
                    Ok(self)
                }

                pub fn build(self) -> flatpg::node::NewNode<#schema_ty> {
                    // TODO: add validation if mandatory fields have not been set
                    self.0
                }
            }
        }
    });

    let gnode_variants = variants.iter().map(|(v, struct_name)| {
        quote! {
            #v(#struct_name<'a>)
        }
    });

    let gnode_new_variants = variants.iter().map(|(v, struct_name)| {
        quote! {
            #ident::#v => Self::#v(#struct_name::new(graph, seq))
        }
    });

    let match_gnode = variants
        .iter()
        .map(|(v, _)| {
            quote! {
                GNode::#v(node)
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        pub mod builders {
            use super::*;
            #(
                #builders_def
            )*
        }

        #structs

        pub enum GNode<'a> {
            #(
                #gnode_variants,
            )*
        }

        impl<'a> GNode<'a> {
            pub fn new(graph: &'a flatpg::graph::Graph<#schema_ty>, kind: #ident, seq: usize) -> Self {
                match kind {
                    #(
                        #gnode_new_variants,
                    )*
                }
            }
        }

        impl<'a> flatpg::node::StoredNode<#schema_ty> for GNode<'a> {

            fn graph(&self) -> &flatpg::graph::Graph<#schema_ty> {
                match self {
                    #(
                        #match_gnode => node.graph(),
                    )*
                }
            }
            fn seq(&self) -> usize {
                match self {
                    #(
                        #match_gnode => node.seq(),
                    )*
                }
            }
            fn kind(&self) -> #ident {
                match self {
                    #(
                        #match_gnode => node.kind(),
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
        impl AvailableProperties<#property_kind_ty> for #name {
            fn properties(&self) -> &'static [#property_kind_ty] {
                match self {
                    #(#props_match_arms,)*
                }
            }
        }
        #(#prop_traits_impls)*
    })
}
