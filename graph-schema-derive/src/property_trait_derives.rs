use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, Ident, ItemEnum, Variant};

use crate::common::{method_ident, typ_last_segment_name};
use crate::enum_derives::{
    PROPERTY_ATTR, PropertyItemAttrs, absent_attribute_error, find_attribute, parse_property_attr,
};

pub(crate) const TYP_NONE: &str = "None";
pub(crate) const TYP_BOOL: &str = "Bool";
pub(crate) const TYP_BYTE: &str = "Byte";
pub(crate) const TYP_SHORT: &str = "Short";
pub(crate) const TYP_INT: &str = "Int";
pub(crate) const TYP_LONG: &str = "Long";
pub(crate) const TYP_FLOAT: &str = "Float";
pub(crate) const TYP_DOUBLE: &str = "Double";
pub(crate) const TYP_NODE_REF: &str = "NodeRef";
pub(crate) const TYP_STRING: &str = "String";
pub(crate) const TYP_ENUM: &str = "Enum";
pub(crate) const QTY_MULTI: &str = "Multi";

pub fn property_traits_derive(input: &ItemEnum) -> TokenStream {
    let enum_ident = &input.ident;
    let vis = &input.vis;

    let parsed = input
        .variants
        .iter()
        .map(
            |Variant {
                 ident: variant,
                 attrs,
                 ..
             }| {
                find_attribute(PROPERTY_ATTR, attrs)
                    .ok_or_else(|| absent_attribute_error(variant, true))
                    .and_then(|attr| parse_property_attr(attr, variant))
            },
        )
        .collect::<Result<Vec<_>, _>>();

    let parsed = match parsed {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };

    let traits = parsed
        .iter()
        .map(|attrs| build_property_trait(enum_ident, vis, attrs))
        .collect::<Result<Vec<_>, _>>();

    match traits {
        Ok(defs) => quote! { #(#defs)* },
        Err(e) => e.to_compile_error(),
    }
}

/// The pieces needed to generate a property accessor method for one `typ`: the element type it
/// returns, the `StoredProperty` pattern that matches a stored value of that type, the expression
/// that converts a match into the return value, and the `PropertyType` to report on a mismatch.
pub(crate) struct PropertyBinding {
    pub(crate) elem_ty: TokenStream,
    pub(crate) pattern: TokenStream,
    pub(crate) expr: TokenStream,
    pub(crate) prop_type_path: TokenStream,
}

pub(crate) fn property_binding(
    typ_name: &str,
    typ: &syn::TypePath,
    schema_ty: &TokenStream,
) -> Result<PropertyBinding, Error> {
    Ok(match typ_name {
        TYP_BOOL => PropertyBinding {
            elem_ty: quote!(bool),
            pattern: quote!(flatpg::storage::StoredProperty::Bool(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Bool),
        },
        TYP_BYTE => PropertyBinding {
            elem_ty: quote!(u8),
            pattern: quote!(flatpg::storage::StoredProperty::Byte(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Byte),
        },
        TYP_SHORT => PropertyBinding {
            elem_ty: quote!(i16),
            pattern: quote!(flatpg::storage::StoredProperty::Short(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Short),
        },
        TYP_INT => PropertyBinding {
            elem_ty: quote!(i32),
            pattern: quote!(flatpg::storage::StoredProperty::Int(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Int),
        },
        TYP_LONG => PropertyBinding {
            elem_ty: quote!(i64),
            pattern: quote!(flatpg::storage::StoredProperty::Long(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Long),
        },
        TYP_FLOAT => PropertyBinding {
            elem_ty: quote!(f32),
            pattern: quote!(flatpg::storage::StoredProperty::Float(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Float),
        },
        TYP_DOUBLE => PropertyBinding {
            elem_ty: quote!(f64),
            pattern: quote!(flatpg::storage::StoredProperty::Double(v)),
            expr: quote!(Ok(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::Double),
        },
        TYP_NODE_REF => PropertyBinding {
            elem_ty: quote!(flatpg::node::NodeId<#schema_ty>),
            pattern: quote!(flatpg::storage::StoredProperty::NodeRef(v)),
            expr: quote!(flatpg::node::NodeId::<#schema_ty>::try_from(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::NodeRef),
        },
        TYP_STRING => PropertyBinding {
            elem_ty: quote!(&str),
            pattern: quote!(flatpg::storage::StoredProperty::StringRef(v)),
            expr: quote!(self.graph().resolve_string(v)),
            prop_type_path: quote!(flatpg::property::PropertyType::String),
        },
        TYP_ENUM => {
            let last_seg = typ
                .path
                .segments
                .last()
                .ok_or_else(|| Error::new_spanned(typ, "expected a non-empty `typ` path"))?;
            let syn::PathArguments::AngleBracketed(args) = &last_seg.arguments else {
                return Err(Error::new_spanned(
                    typ,
                    "`Enum` typ requires a single generic argument naming the registered enum \
                     type, e.g. `Enum<Status>`",
                ));
            };
            let mut inner_types = args.args.iter().filter_map(|a| match a {
                syn::GenericArgument::Type(t) => Some(t),
                _ => None,
            });
            let (Some(inner_ty), None) = (inner_types.next(), inner_types.next()) else {
                return Err(Error::new_spanned(
                    typ,
                    "`Enum` typ requires exactly one type argument, e.g. `Enum<Status>`",
                ));
            };
            let enum_name = quote!(#inner_ty).to_string();
            PropertyBinding {
                elem_ty: quote!(#inner_ty),
                pattern: quote!(flatpg::storage::StoredProperty::Enum(v)),
                expr: quote! {
                    if v.enum_property_index() == <#inner_ty as EnumPropertyIndex>::enum_property_index() {
                        <#inner_ty as ItemFromIndex>::from_index(v.variant()).ok_or_else(|| {
                            flatpg::error::Error::unresolved_enum_variant(#enum_name, v.variant())
                        })
                    } else {
                        Err(flatpg::error::Error::enum_property_index_mismatch(#enum_name, v.enum_property_index()))
                    }
                },
                prop_type_path: quote!(flatpg::property::PropertyType::Enum),
            }
        }
        other => {
            return Err(Error::new_spanned(
                typ,
                format!("unsupported property typ `{other}`"),
            ));
        }
    })
}

fn build_property_trait(
    enum_ident: &Ident,
    vis: &syn::Visibility,
    attrs: &PropertyItemAttrs,
) -> Result<TokenStream, Error> {
    let variant = &attrs.variant;
    let typ = attrs
        .prop_typ
        .as_ref()
        .ok_or_else(|| absent_attribute_error(variant, true))?;
    let qty = attrs
        .prop_qty
        .as_ref()
        .ok_or_else(|| absent_attribute_error(variant, true))?;

    let typ_name = typ_last_segment_name(typ)?;

    if typ_name == TYP_NONE {
        return Err(Error::new_spanned(
            variant,
            format!(
                "#[property(typ = None)] is not valid on a node-property enum variant \
                 (`{variant}`); `None` is reserved for edge property kinds that carry no \
                 value. Use one of: Bool, Byte, Short, Int, Long, Float, Double, NodeRef, \
                 String."
            ),
        ));
    }

    let is_multi = qty
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .map(|s| s == QTY_MULTI)
        .unwrap_or_default();

    let method_name = method_ident(variant);

    let PropertyBinding {
        elem_ty,
        pattern,
        expr,
        prop_type_path,
    } = property_binding(&typ_name, typ, &quote!(S))?;

    let (generics, self_param, where_clause) = if typ_name == TYP_STRING {
        (quote!(<'a>), quote!(&'a self), quote!(where S: 'a))
    } else {
        (quote!(), quote!(&self), quote!())
    };

    let method = if is_multi {
        quote! {
            fn #method_name #generics (#self_param) -> Result<Vec<#elem_ty>, flatpg::error::Error>
            #where_clause
            {
                self.graph()
                    .get_node_property(self.node_ref(), #enum_ident::#variant)?
                    .map(|p| match p {
                        #pattern => #expr,
                        other => Err(flatpg::error::Error::invalid_property_type(#prop_type_path, other.typ())),
                    })
                    .collect()
            }
        }
    } else {
        quote! {
            fn #method_name #generics (#self_param) -> Result<#elem_ty, flatpg::error::Error>
            #where_clause
            {
                self.graph()
                    .get_node_property(self.node_ref(), #enum_ident::#variant)
                    .and_then(|mut p| match p.next() {
                        Some(#pattern) => #expr,
                        Some(other) => Err(flatpg::error::Error::invalid_property_type(#prop_type_path, other.typ())),
                        None => Err(flatpg::error::Error::property_index_not_found()),
                    })
            }
        }
    };

    Ok(quote! {
        #vis trait #variant<S: flatpg::schema::Schema<P = #enum_ident>>: flatpg::node::StoredNode<S> {
            #method
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{
        has_compile_error, parse_enum, parse_output, return_type_string,
    };
    use syn::{File, ItemTrait, TraitItemFn};

    const IDENT_SCHEMA: &str = "Schema";
    const IDENT_STORED_NODE: &str = "StoredNode";

    fn find_trait<'a>(file: &'a File, name: &str) -> Option<&'a ItemTrait> {
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

    #[test]
    fn generates_one_trait_per_variant() {
        let input = parse_enum(
            r#"enum P {
                #[property(typ = Int, quantity = One)] Count,
                #[property(typ = String, quantity = Multi)] Tags,
            }"#,
        );
        let file = parse_output(property_traits_derive(&input));

        let count_trait = find_trait(&file, "Count").expect("Count trait not found");
        assert!(find_trait_method(count_trait, "count").is_some());

        let tags_trait = find_trait(&file, "Tags").expect("Tags trait not found");
        assert!(find_trait_method(tags_trait, "tags").is_some());
    }

    #[test]
    fn escapes_keyword_collision() {
        let input = parse_enum(r#"enum P { #[property(typ = NodeRef, quantity = One)] Ref }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "Ref").expect("Ref trait not found");
        // Exactly one method, whose unraw name is "ref" (i.e. it was raw-escaped).
        assert_eq!(t.items.len(), 1);
        let syn::TraitItem::Fn(f) = &t.items[0] else {
            panic!("expected fn")
        };
        use syn::ext::IdentExt;
        assert_eq!(f.sig.ident.unraw().to_string(), "ref");
    }

    #[test]
    fn one_quantity_returns_bare_result() {
        let input = parse_enum(r#"enum P { #[property(typ = Int, quantity = One)] Count }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "Count").unwrap();
        let m = find_trait_method(t, "count").unwrap();
        let ret = return_type_string(&m.sig);
        assert!(ret.contains("Result"));
        assert!(ret.contains("i32"));
        assert!(!ret.contains("Vec"));
    }

    #[test]
    fn multi_quantity_returns_vec_result() {
        let input = parse_enum(r#"enum P { #[property(typ = String, quantity = Multi)] Tags }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "Tags").unwrap();
        let m = find_trait_method(t, "tags").unwrap();
        let ret = return_type_string(&m.sig);
        assert!(ret.contains("Vec"));
        assert!(ret.contains("str"));
    }

    #[test]
    fn node_ref_typed_property_returns_node() {
        let input = parse_enum(r#"enum P { #[property(typ = NodeRef, quantity = One)] Owner }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "Owner").unwrap();
        let m = find_trait_method(t, "owner").unwrap();
        let ret = return_type_string(&m.sig);
        assert!(ret.contains("NodeId"));
    }

    #[test]
    fn enum_typed_property_one_quantity_returns_bare_enum() {
        let input =
            parse_enum(r#"enum P { #[property(typ = Enum<Status>, quantity = One)] State }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "State").unwrap();
        let m = find_trait_method(t, "state").unwrap();
        let ret = return_type_string(&m.sig);
        assert!(ret.contains("Status"));
        assert!(!ret.contains("Vec"));
    }

    #[test]
    fn enum_typed_property_multi_quantity_returns_vec() {
        let input =
            parse_enum(r#"enum P { #[property(typ = Enum<Status>, quantity = Multi)] States }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "States").unwrap();
        let m = find_trait_method(t, "states").unwrap();
        let ret = return_type_string(&m.sig);
        assert!(ret.contains("Vec"));
        assert!(ret.contains("Status"));
    }

    #[test]
    fn enum_typed_property_missing_generic_argument_errors() {
        let input = parse_enum(r#"enum P { #[property(typ = Enum, quantity = One)] State }"#);
        assert!(has_compile_error(property_traits_derive(&input)));
    }

    #[test]
    fn enum_typed_property_multiple_generic_arguments_error() {
        let input = parse_enum(
            r#"enum P { #[property(typ = Enum<Status, Color>, quantity = One)] State }"#,
        );
        assert!(has_compile_error(property_traits_derive(&input)));
    }

    #[test]
    fn trait_bound_shape() {
        let input = parse_enum(r#"enum P { #[property(typ = Int, quantity = One)] Count }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "Count").unwrap();
        assert_eq!(t.generics.params.len(), 1);
        let syn::GenericParam::Type(tp) = &t.generics.params[0] else {
            panic!("expected type param")
        };
        assert!(tp.bounds.iter().any(|b| {
            let syn::TypeParamBound::Trait(tb) = b else {
                return false;
            };
            tb.path
                .segments
                .last()
                .map(|s| s.ident == IDENT_SCHEMA)
                .unwrap_or(false)
        }));
        assert!(t.supertraits.iter().any(|b| {
            let syn::TypeParamBound::Trait(tb) = b else {
                return false;
            };
            tb.path
                .segments
                .last()
                .map(|s| s.ident == IDENT_STORED_NODE)
                .unwrap_or(false)
        }));
    }

    #[test]
    fn rejects_typ_none() {
        let input = parse_enum(r#"enum P { #[property(typ = None, quantity = One)] X }"#);
        assert!(has_compile_error(property_traits_derive(&input)));
    }

    #[test]
    fn rejects_missing_attribute() {
        let input = parse_enum("enum P { X }");
        assert!(has_compile_error(property_traits_derive(&input)));
    }
}
