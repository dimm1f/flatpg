use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, Ident, ItemEnum, Variant};

use crate::enum_derives::{
    PROPERTY_ATTR, PropertyItemAttrs, absent_attribute_error, find_attribute, parse_property_attr,
};

const TYP_NONE: &str = "None";
const TYP_BOOL: &str = "Bool";
const TYP_BYTE: &str = "Byte";
const TYP_SHORT: &str = "Short";
const TYP_INT: &str = "Int";
const TYP_LONG: &str = "Long";
const TYP_FLOAT: &str = "Float";
const TYP_DOUBLE: &str = "Double";
const TYP_NODE_REF: &str = "NodeRef";
const TYP_STRING: &str = "String";
const QTY_MULTI: &str = "Multi";

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
    "typeof", "unsized", "virtual", "yield", "try",
];

fn is_rust_keyword(s: &str) -> bool {
    RUST_KEYWORDS.contains(&s)
}

/// Converts a PascalCase identifier (an enum variant) into a snake_case string
/// suitable for a method name, e.g. `FullName` -> `full_name`, `HTTPServer` ->
/// `http_server`.
fn to_snake_case(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 4);
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() {
            let prev_lower_or_digit =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let prev_upper_next_lower = i > 0
                && chars[i - 1].is_ascii_uppercase()
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_lowercase();
            if i > 0 && (prev_lower_or_digit || prev_upper_next_lower) {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Builds the accessor method identifier for a variant, escaping the (rare)
/// case where the snake_case form collides with a reserved Rust keyword
/// (e.g. variant `Ref` -> method `r#ref`) via a raw identifier.
fn method_ident(variant: &Ident) -> Ident {
    let snake = to_snake_case(&variant.to_string());
    if is_rust_keyword(&snake) {
        Ident::new_raw(&snake, variant.span())
    } else {
        Ident::new(&snake, variant.span())
    }
}

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

    let typ_name = typ
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .ok_or_else(|| Error::new_spanned(typ, "expected a non-empty `typ` path"))?;

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

    let (elem_ty, pattern, expr, prop_type_path) = match typ_name.as_str() {
        TYP_BOOL => (
            quote!(bool),
            quote!(flatpg::storage::StoredProperty::Bool(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Bool),
        ),
        TYP_BYTE => (
            quote!(u8),
            quote!(flatpg::storage::StoredProperty::Byte(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Byte),
        ),
        TYP_SHORT => (
            quote!(i16),
            quote!(flatpg::storage::StoredProperty::Short(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Short),
        ),
        TYP_INT => (
            quote!(i32),
            quote!(flatpg::storage::StoredProperty::Int(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Int),
        ),
        TYP_LONG => (
            quote!(i64),
            quote!(flatpg::storage::StoredProperty::Long(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Long),
        ),
        TYP_FLOAT => (
            quote!(f32),
            quote!(flatpg::storage::StoredProperty::Float(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Float),
        ),
        TYP_DOUBLE => (
            quote!(f64),
            quote!(flatpg::storage::StoredProperty::Double(v)),
            quote!(Ok(v)),
            quote!(flatpg::property::PropertyType::Double),
        ),
        TYP_NODE_REF => (
            quote!(flatpg::node::Node<S>),
            quote!(flatpg::storage::StoredProperty::NodeRef(v)),
            quote!(flatpg::node::Node::<S>::try_from(v)),
            quote!(flatpg::property::PropertyType::NodeRef),
        ),
        TYP_STRING => (
            quote!(&str),
            quote!(flatpg::storage::StoredProperty::StringRef(v)),
            quote!(self.graph().resolve_string(v)),
            quote!(flatpg::property::PropertyType::String),
        ),
        other => {
            return Err(Error::new_spanned(
                typ,
                format!("unsupported property typ `{other}`"),
            ));
        }
    };

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
    use syn::{File, ItemTrait, TraitItemFn, parse_str, parse2};

    const IDENT_COMPILE_ERROR: &str = "compile_error";
    const IDENT_SCHEMA: &str = "Schema";
    const IDENT_STORED_NODE: &str = "StoredNode";

    fn parse_enum(src: &str) -> ItemEnum {
        parse_str(src).expect("failed to parse enum")
    }

    fn parse_output(ts: TokenStream) -> File {
        parse2(ts).expect("generated output is not valid Rust")
    }

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

    fn has_compile_error(ts: TokenStream) -> bool {
        parse2::<File>(ts).is_ok_and(|file| {
            file.items.iter().any(|item| {
                let syn::Item::Macro(m) = item else {
                    return false;
                };
                m.mac
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == IDENT_COMPILE_ERROR)
            })
        })
    }

    fn return_type_string(f: &TraitItemFn) -> String {
        let syn::ReturnType::Type(_, ty) = &f.sig.output else {
            panic!("expected a return type")
        };
        quote::quote!(#ty).to_string()
    }

    #[test]
    fn to_snake_case_basic() {
        assert_eq!(to_snake_case("FullName"), "full_name");
        assert_eq!(to_snake_case("Key"), "key");
        assert_eq!(to_snake_case("Ref"), "ref");
        assert_eq!(to_snake_case("HTTPServer"), "http_server");
        assert_eq!(to_snake_case("File01"), "file01");
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
        let ret = return_type_string(m);
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
        let ret = return_type_string(m);
        assert!(ret.contains("Vec"));
        assert!(ret.contains("str"));
    }

    #[test]
    fn node_ref_typed_property_returns_node() {
        let input = parse_enum(r#"enum P { #[property(typ = NodeRef, quantity = One)] Owner }"#);
        let file = parse_output(property_traits_derive(&input));
        let t = find_trait(&file, "Owner").unwrap();
        let m = find_trait_method(t, "owner").unwrap();
        let ret = return_type_string(m);
        assert!(ret.contains("Node"));
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
