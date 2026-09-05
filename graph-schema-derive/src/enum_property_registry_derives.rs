use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, Ident, ItemEnum, TypePath};

use crate::enum_derives::find_attribute;

const ENUM_TYPE_ATTR: &str = "enum_type";

fn missing_enum_type_error(variant: &Ident) -> Error {
    Error::new_spanned(
        variant,
        format!(
            "missing required attribute: #[{ENUM_TYPE_ATTR}(<EnumType>)] — every variant of a \
             #[derive(EnumPropertyRegistry)] registry enum must name the domain enum it registers, \
             e.g. #[{ENUM_TYPE_ATTR}(Status)]"
        ),
    )
}

fn parse_enum_type_attr(input: &ItemEnum) -> Result<Vec<TypePath>, Error> {
    input
        .variants
        .iter()
        .map(|variant| {
            let attr = find_attribute(ENUM_TYPE_ATTR, &variant.attrs)
                .ok_or_else(|| missing_enum_type_error(&variant.ident))?;
            attr.parse_args::<TypePath>()
        })
        .collect()
}

pub fn enum_property_registry_derive(input: &ItemEnum) -> TokenStream {
    let enum_types = match parse_enum_type_attr(input) {
        Ok(types) => types,
        Err(e) => return e.to_compile_error(),
    };

    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let variant_idents: Vec<&Ident> = input.variants.iter().map(|v| &v.ident).collect();

    let variant_count_arms =
        variant_idents
            .iter()
            .zip(enum_types.iter())
            .map(|(variant_ident, enum_ty)| {
                quote! {
                    Self::#variant_ident => <#enum_ty as ::flatpg::prelude::ItemAll>::all().len(),
                }
            });

    let impls = enum_types.iter().enumerate().map(|(i, enum_ty)| {
        let enum_name = quote!(#enum_ty).to_string();
        quote! {
            #[automatically_derived]
            impl ::flatpg::prelude::EnumPropertyIndex for #enum_ty {
                fn enum_property_index() -> usize {
                    #i
                }
            }

            #[automatically_derived]
            impl ::core::convert::TryFrom<::flatpg::property::PropertyValue> for #enum_ty {
                type Error = ::flatpg::error::Error;
                fn try_from(
                    value: ::flatpg::property::PropertyValue,
                ) -> ::core::result::Result<Self, Self::Error> {
                    match value {
                        ::flatpg::property::PropertyValue::Enum(v)
                            if v.enum_property_index()
                                == <#enum_ty as ::flatpg::prelude::EnumPropertyIndex>::enum_property_index() =>
                        {
                            <#enum_ty as ::flatpg::prelude::ItemFromIndex>::from_index(v.variant())
                                .ok_or_else(|| {
                                    ::flatpg::error::Error::unresolved_enum_variant(#enum_name, v.variant())
                                })
                        }
                        ::flatpg::property::PropertyValue::Enum(v) => ::core::result::Result::Err(
                            ::flatpg::error::Error::enum_property_index_mismatch(#enum_name, v.enum_property_index()),
                        ),
                        other => ::core::result::Result::Err(
                            ::flatpg::error::Error::invalid_property_type(
                                ::flatpg::property::PropertyType::Enum,
                                other.typ(),
                            ),
                        ),
                    }
                }
            }
        }
    });

    quote! {
        #(#impls)*

        #[automatically_derived]
        impl #impl_generics ::flatpg::prelude::EnumPropertyRegistry for #ident #ty_generics #where_clause {
            fn variant_count(&self) -> usize {
                match *self {
                    #(#variant_count_arms)*
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{find_impl, has_compile_error, parse_enum, parse_output};

    #[test]
    fn enum_property_index_matches_variant_position() {
        let input = parse_enum(
            r#"enum PropEnumsRegistry {
                #[enum_type(Status)] Status,
                #[enum_type(Color)] Color,
            }"#,
        );
        let file = parse_output(enum_property_registry_derive(&input));

        let status_impl = find_impl(&file, "EnumPropertyIndex", "Status")
            .expect("impl EnumPropertyIndex for Status");
        let color_impl = find_impl(&file, "EnumPropertyIndex", "Color")
            .expect("impl EnumPropertyIndex for Color");

        let extract_index = |impl_block: &syn::ItemImpl| {
            let syn::ImplItem::Fn(f) = &impl_block.items[0] else {
                panic!("expected fn")
            };
            let syn::Stmt::Expr(syn::Expr::Lit(lit), _) = &f.block.stmts[0] else {
                panic!("expected literal return")
            };
            let syn::Lit::Int(i) = &lit.lit else {
                panic!("expected int literal")
            };
            i.base10_parse::<usize>().unwrap()
        };

        assert_eq!(extract_index(status_impl), 0);
        assert_eq!(extract_index(color_impl), 1);
    }

    #[test]
    fn generates_try_from_property_value() {
        let input = parse_enum(r#"enum PropEnumsRegistry { #[enum_type(Status)] Status }"#);
        let file = parse_output(enum_property_registry_derive(&input));

        assert!(find_impl(&file, "TryFrom", "Status").is_some());
    }

    #[test]
    fn missing_enum_type_attribute_emits_compile_error() {
        let input = parse_enum("enum PropEnumsRegistry { Status }");
        assert!(has_compile_error(enum_property_registry_derive(&input)));
    }

    #[test]
    fn registry_impl_forwards_generics_from_input_enum() {
        let input =
            parse_enum(r#"enum PropEnumsRegistry<const N: usize> { #[enum_type(Status)] Status }"#);
        let file = parse_output(enum_property_registry_derive(&input));

        let impl_block = find_impl(&file, "EnumPropertyRegistry", "PropEnumsRegistry")
            .expect("impl EnumPropertyRegistry for PropEnumsRegistry not found");
        assert_eq!(impl_block.generics.params.len(), 1);
    }
}
