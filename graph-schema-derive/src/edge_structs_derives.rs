use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, Ident, ItemEnum, TypePath, Variant};

use crate::common::{SCHEMA_PARAM, parse_kind_attr, typ_last_segment_name};
use crate::enum_derives::{
    PROPERTY_ATTR, PropertyItemAttrs, absent_attribute_error, find_attribute, parse_property_attr,
    require_type_param,
};
use crate::property_trait_derives::{TYP_NONE, TYP_STRING, property_binding};

const EDGE_KIND_ATTR: &str = "edge_kind";

pub struct EdgeKindConfig {
    pub schema: TypePath,
}

pub fn parse_edge_kind_config(input: &ItemEnum) -> Result<EdgeKindConfig, Error> {
    let (attr, args) = parse_kind_attr(EDGE_KIND_ATTR, input, &format!("{SCHEMA_PARAM} = ..."))?;

    Ok(EdgeKindConfig {
        schema: require_type_param(attr, &args, SCHEMA_PARAM, EDGE_KIND_ATTR)?,
    })
}

fn edge_struct_name(name: &Ident) -> Ident {
    format_ident!("{}Edge", name)
}

fn build_edge_property_method(
    attrs: &PropertyItemAttrs,
    schema_ty: &TypePath,
) -> Result<TokenStream, Error> {
    let variant = &attrs.variant;
    let typ = attrs
        .prop_typ
        .as_ref()
        .ok_or_else(|| absent_attribute_error(variant, false))?;
    let typ_name = typ_last_segment_name(typ)?;

    if typ_name == TYP_NONE {
        return Ok(quote!());
    }

    let (elem_ty, pattern, expr, prop_type_path) =
        property_binding(&typ_name, typ, &quote!(#schema_ty))?;

    let self_param = if typ_name == TYP_STRING {
        quote!(&'a self)
    } else {
        quote!(&self)
    };

    Ok(quote! {
        pub fn property(#self_param) -> Result<Option<#elem_ty>, flatpg::error::Error> {
            self.graph()
                .get_edge_property(self.edge())?
                .map(|p| match p {
                    #pattern => #expr,
                    other => Err(flatpg::error::Error::invalid_property_type(#prop_type_path, other.typ())),
                })
                .transpose()
        }
    })
}

fn expand_edge_structs(
    name: &Ident,
    vars: &[(&Ident, Ident, PropertyItemAttrs)],
    config: &EdgeKindConfig,
) -> Result<TokenStream, Error> {
    let schema_ty = &config.schema;
    let structs = vars
        .iter()
        .map(|(v, struct_name, attrs)| {
            let property_method = build_edge_property_method(attrs, schema_ty)?;
            Ok(quote! {
                pub struct #struct_name<'a> {
                    graph: &'a flatpg::graph::Graph<#schema_ty>,
                    src_node: flatpg::node::NodeId<#schema_ty>,
                    dst_node: flatpg::node::NodeId<#schema_ty>,
                    direction: flatpg::edge::Direction,
                    seq: usize,
                }

                impl<'a> #struct_name<'a> {
                    pub fn new(
                        graph: &'a flatpg::graph::Graph<#schema_ty>,
                        src_node: flatpg::node::NodeId<#schema_ty>,
                        dst_node: flatpg::node::NodeId<#schema_ty>,
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

                    #property_method
                }

                impl<'a> flatpg::edge::StoredEdge<#schema_ty> for #struct_name<'a> {
                    #[inline]
                    fn graph(&self) -> &flatpg::graph::Graph<#schema_ty> {
                        self.graph
                    }
                    #[inline]
                    fn src_node(&self) -> flatpg::node::NodeId<#schema_ty> {
                        self.src_node
                    }
                    #[inline]
                    fn dst_node(&self) -> flatpg::node::NodeId<#schema_ty> {
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
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(quote! {
        #(
            #structs
        )*
    })
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
        .map(
            |Variant {
                 ident: variant,
                 attrs,
                 ..
             }| {
                let attr = find_attribute(PROPERTY_ATTR, attrs)
                    .ok_or_else(|| absent_attribute_error(variant, false))?;
                let parsed = parse_property_attr(attr, variant)?;
                Ok((variant, edge_struct_name(variant), parsed))
            },
        )
        .collect::<Result<Vec<_>, Error>>()?;

    let structs = expand_edge_structs(ident, &variants, config)?;

    let edge_variants = variants.iter().map(|(v, struct_name, _)| {
        quote! {
            #v(#struct_name<'a>)
        }
    });

    let edge_new_variants = variants.iter().map(|(v, struct_name, _)| {
        quote! {
            #ident::#v => Self::#v(#struct_name::new(graph, src_node, dst_node, direction, seq))
        }
    });

    let match_edge = variants
        .iter()
        .map(|(v, _, _)| {
            quote! {
                Edge::#v(edge)
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        #structs

        pub enum Edge<'a> {
            #(
                #edge_variants,
            )*
        }

        impl<'a> Edge<'a> {
            pub fn new(
                graph: &'a flatpg::graph::Graph<#schema_ty>,
                kind: #ident,
                src_node: flatpg::node::NodeId<#schema_ty>,
                dst_node: flatpg::node::NodeId<#schema_ty>,
                direction: flatpg::edge::Direction,
                seq: usize,
            ) -> Self {
                match kind {
                    #(
                        #edge_new_variants,
                    )*
                }
            }
        }

        impl<'a> flatpg::edge::StoredEdge<#schema_ty> for Edge<'a> {
            fn graph(&self) -> &flatpg::graph::Graph<#schema_ty> {
                match self {
                    #(
                        #match_edge => edge.graph(),
                    )*
                }
            }
            fn src_node(&self) -> flatpg::node::NodeId<#schema_ty> {
                match self {
                    #(
                        #match_edge => edge.src_node(),
                    )*
                }
            }
            fn dst_node(&self) -> flatpg::node::NodeId<#schema_ty> {
                match self {
                    #(
                        #match_edge => edge.dst_node(),
                    )*
                }
            }
            fn direction(&self) -> flatpg::edge::Direction {
                match self {
                    #(
                        #match_edge => edge.direction(),
                    )*
                }
            }
            fn seq(&self) -> usize {
                match self {
                    #(
                        #match_edge => edge.seq(),
                    )*
                }
            }
            fn kind(&self) -> #ident {
                match self {
                    #(
                        #match_edge => edge.kind(),
                    )*
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{
        find_impl, find_method, match_arm_count, parse_enum, parse_output, return_type_string,
    };
    use syn::File;

    fn find_inherent_impl<'a>(file: &'a File, self_type: &str) -> Option<&'a syn::ItemImpl> {
        file.items.iter().find_map(|item| {
            let syn::Item::Impl(impl_block) = item else {
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

    fn find_struct<'a>(file: &'a File, name: &str) -> Option<&'a syn::ItemStruct> {
        file.items.iter().find_map(|item| {
            let syn::Item::Struct(s) = item else {
                return None;
            };
            (s.ident == name).then_some(s)
        })
    }

    fn find_enum<'a>(file: &'a File, name: &str) -> Option<&'a syn::ItemEnum> {
        file.items.iter().find_map(|item| {
            let syn::Item::Enum(e) = item else {
                return None;
            };
            (e.ident == name).then_some(e)
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
    fn edge_structs_derive_generates_edge_enum_with_variant_per_input() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] A,
                #[property(typ = String)] B,
                #[property(typ = Int)] C,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let edge_enum = find_enum(&file, "Edge").expect("Edge enum not found");
        assert_eq!(edge_enum.variants.len(), 3);
    }

    #[test]
    fn edge_structs_derive_edge_new_and_stored_edge_impl_match_arm_counts() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] A,
                #[property(typ = String)] B,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let edge_impl =
            find_inherent_impl(&file, "Edge").expect("inherent impl for Edge not found");
        let new_method = find_method(edge_impl, "new").expect("Edge::new not found");
        assert_eq!(match_arm_count(new_method), Some(2));

        let stored_edge =
            find_impl(&file, "StoredEdge", "Edge").expect("StoredEdge impl for Edge not found");
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

    #[test]
    fn property_method_generated_for_typed_variant() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] Plain,
                #[property(typ = String)] Labeled,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let inherent = find_inherent_impl(&file, "LabeledEdge")
            .expect("inherent impl for LabeledEdge not found");
        assert!(find_method(inherent, "property").is_some());
    }

    #[test]
    fn property_method_absent_for_none_typed_variant() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] Plain,
                #[property(typ = String)] Labeled,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let inherent =
            find_inherent_impl(&file, "PlainEdge").expect("inherent impl for PlainEdge not found");
        assert!(find_method(inherent, "property").is_none());
    }

    #[test]
    fn property_method_return_type_per_typ() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = String)] Labeled,
                #[property(typ = Int)] Weighted,
                #[property(typ = NodeRef)] Linked,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let labeled = find_method(
            find_inherent_impl(&file, "LabeledEdge").unwrap(),
            "property",
        )
        .unwrap();
        let labeled_ret = return_type_string(&labeled.sig);
        assert!(labeled_ret.contains("& str"));
        assert!(!labeled_ret.contains("String"));

        let weighted = find_method(
            find_inherent_impl(&file, "WeightedEdge").unwrap(),
            "property",
        )
        .unwrap();
        let weighted_ret = return_type_string(&weighted.sig);
        assert!(weighted_ret.contains("i32"));
        assert!(!weighted_ret.contains('&'));

        let linked =
            find_method(find_inherent_impl(&file, "LinkedEdge").unwrap(), "property").unwrap();
        let linked_ret = return_type_string(&linked.sig);
        assert!(linked_ret.contains("NodeId"));
        assert!(linked_ret.contains("MySchema"));
    }

    #[test]
    fn edge_has_no_property_method() {
        let input = parse_enum(
            r#"enum E {
                #[property(typ = None)] Plain,
                #[property(typ = String)] Labeled,
            }"#,
        );
        let config = edge_kind_config("MySchema");
        let file = parse_output(edge_structs_derive(&input, &config).unwrap());

        let edge_impl =
            find_inherent_impl(&file, "Edge").expect("inherent impl for Edge not found");
        assert!(find_method(edge_impl, "property").is_none());

        let stored_edge =
            find_impl(&file, "StoredEdge", "Edge").expect("StoredEdge impl for Edge not found");
        assert!(find_method(stored_edge, "property").is_none());
    }
}
