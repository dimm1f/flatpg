use proc_macro2::Ident;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Attribute, Error, ItemEnum, LitStr, TypePath, Variant, parse2, punctuated::Punctuated};

pub(crate) const PROPERTY_ATTR: &str = "property";
const PROPERTY_TYPE_KEY: &str = "typ";
const PROPERTY_QTY_KEY: &str = "quantity";

pub(crate) struct PropertyItemAttrs {
    pub(crate) variant: Ident,
    pub(crate) prop_typ: Option<TypePath>,
    pub(crate) prop_qty: Option<TypePath>,
}

pub(crate) fn find_attribute<'a>(ident_str: &str, attrs: &'a [Attribute]) -> Option<&'a Attribute> {
    attrs.iter().find(|a| a.path().is_ident(ident_str))
}

pub(crate) fn parse_comma_separated_types(
    attr: &Attribute,
) -> Result<Punctuated<TypePath, syn::token::Comma>, Error> {
    attr.parse_args_with(Punctuated::<syn::TypePath, syn::Token![,]>::parse_terminated)
}

pub fn enum_item_all_derive(input: &ItemEnum) -> TokenStream {
    let ident = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let variants = input
        .variants
        .iter()
        .map(|Variant { ident: variant, .. }| {
            quote! {#ident::#variant}
        })
        .collect::<Vec<_>>();
    let variants_count = variants.len();
    let items_array_name = format_ident!("_{}_ITEMS", &ident.to_string().to_uppercase());

    quote! {
        const #items_array_name: [#ident; #variants_count] = [#(#variants,)*];
        #[automatically_derived]
        impl #impl_generics ItemAll for #ident #ty_generics #where_clause {
            #vis fn all() -> &'static [#ident] {
                &#items_array_name
            }
        }
    }
}

pub fn enum_item_from_index_derive(input: &ItemEnum) -> TokenStream {
    let ident = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let variants = input
        .variants
        .iter()
        .enumerate()
        .map(|(i, Variant { ident: variant, .. })| {
            quote! {#i => Some(#ident::#variant)}
        });

    quote! {
        #[automatically_derived]
        impl #impl_generics ItemFromIndex for #ident #ty_generics #where_clause {
            #vis fn from_index(index: usize) -> Option<Self> {
                match index {
                    #(
                        #variants,
                    )*
                    _ => None,
                }
            }
        }
    }
}

pub fn enum_item_index_derive(input: &ItemEnum) -> TokenStream {
    let ident = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let variants = input
        .variants
        .iter()
        .enumerate()
        .map(|(i, Variant { ident: variant, .. })| {
            quote! {#ident::#variant => #i}
        });

    quote! {
        #[automatically_derived]
        impl #impl_generics ItemIndex for #ident #ty_generics #where_clause {
            #vis fn index(&self) -> usize {
                match self {
                    #(#variants,)*
                }
            }
        }
    }
}

pub fn enum_item_as_str_derive(input: &ItemEnum) -> TokenStream {
    let ident = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let variants = input
        .variants
        .iter()
        .map(|Variant { ident: variant, .. }| variant)
        .collect::<Vec<_>>();

    let orig_variants = variants.iter().map(|variant| {
        let label = LitStr::new(&variant.to_string(), variant.span());
        quote! {#ident::#variant => #label}
    });

    quote! {
        #[automatically_derived]
        impl #impl_generics ItemAsStr for #ident #ty_generics #where_clause {
            #vis fn as_str(&self) -> &'static str {
                match self {
                    #(
                        #orig_variants,
                    )*
                }
            }
        }
    }
}

pub fn enum_item_from_str_derive(input: &ItemEnum) -> TokenStream {
    let ident = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let variants = input
        .variants
        .iter()
        .map(|Variant { ident, .. }| ident)
        .collect::<Vec<_>>();

    let orig_variants = variants.iter().map(|variant| {
        let label = LitStr::new(&variant.to_string(), variant.span());
        quote! {#label => Ok(#ident::#variant)}
    });

    quote! {
        #[automatically_derived]
        impl #impl_generics std::str::FromStr for #ident #ty_generics #where_clause {
            type Err = flatpg::error::Error;
            #vis fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    #(
                        #orig_variants,
                    )*
                    _ => Err(Self::Err::unknown_label(stringify!(#ident), s)),
                }
            }
        }
        impl ItemFromStr for #ident {}
    }
}

pub(crate) fn absent_attribute_error<T>(span: T, has_qty: bool) -> Error
where
    T: ToTokens,
{
    let msg = if has_qty {
        format!(
            "missing required attribute: #[{PROPERTY_ATTR}(typ = Int, quantity = Multi)] \
             — both `typ` and `quantity` are mandatory. \
             Full form: #[{PROPERTY_ATTR}(typ = <PropertyType variant>, quantity = <QuantityType variant>)]"
        )
    } else {
        format!(
            "missing required attribute: #[{PROPERTY_ATTR}(typ = Int)] \
             — `typ` is mandatory. \
             Full form: #[{PROPERTY_ATTR}(typ = <PropertyType variant>)]"
        )
    };
    Error::new_spanned(span, msg)
}

pub(crate) fn parse_property_attr(
    attr: &Attribute,
    variant: &Ident,
) -> Result<PropertyItemAttrs, Error> {
    let prefs =
        attr.parse_args_with(Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated)?;

    let prop_typ: Option<TypePath> = prefs
        .iter()
        .filter(|m| m.path.is_ident(PROPERTY_TYPE_KEY))
        .map(|m| m.value.to_token_stream())
        .next()
        .map(parse2)
        .transpose()?;

    let prop_qty: Option<TypePath> = prefs
        .iter()
        .filter(|m| m.path.is_ident(PROPERTY_QTY_KEY))
        .map(|m| m.value.to_token_stream())
        .next()
        .map(parse2)
        .transpose()?;

    Ok(PropertyItemAttrs {
        prop_typ,
        prop_qty,
        variant: variant.clone(),
    })
}

pub fn item_kind_property_type_derive(input: &ItemEnum, has_qty: bool) -> TokenStream {
    let ident = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    let variant_attrs = input
        .variants
        .iter()
        .map(
            |Variant {
                 ident: variant,
                 attrs,
                 ..
             }| {
                find_attribute(PROPERTY_ATTR, attrs)
                    .ok_or_else(|| absent_attribute_error(variant, has_qty))
                    .and_then(|attr| parse_property_attr(attr, variant))
            },
        )
        .collect::<Result<Vec<_>, _>>();

    let variant_attrs = match variant_attrs {
        Ok(attrs) => attrs,
        Err(e) => return e.to_compile_error(),
    };

    let results: Result<Vec<_>, Error> = variant_attrs
        .iter()
        .map(|attr| {
            let variant = &attr.variant;

            let typ = attr
                .prop_typ
                .as_ref()
                .ok_or_else(|| absent_attribute_error(variant, has_qty))?;
            let typ = quote! {#ident::#variant => flatpg::property::PropertyType::#typ};

            let qty = if has_qty {
                let prop_qty = attr
                    .prop_qty
                    .as_ref()
                    .ok_or_else(|| absent_attribute_error(variant, has_qty))?;
                quote! {#ident::#variant => flatpg::property::QuantityType::#prop_qty}
            } else {
                quote! {#ident::#variant => flatpg::property::QuantityType::One}
            };

            Ok((typ, qty))
        })
        .collect();

    let (prop_type_variants, prop_qty_variants): (Vec<_>, Vec<_>) = match results {
        Ok(v) => v.into_iter().unzip(),
        Err(e) => return e.to_compile_error(),
    };

    quote! {
        #[automatically_derived]
        impl #impl_generics ItemKindPropertyType for #ident #ty_generics #where_clause {
            type PropertyType = flatpg::property::PropertyType;
            type QuantityType = flatpg::property::QuantityType;
            #vis fn property_type(&self) -> Self::PropertyType {
                match self {
                    #(#prop_type_variants,)*
                }
            }
            #vis fn property_quantity(&self) -> Self::QuantityType {
                match self {
                    #(#prop_qty_variants,)*
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Expr, File, ImplItem, Item, Stmt, parse_str, parse2, punctuated::Punctuated};

    fn parse_enum(src: &str) -> ItemEnum {
        parse_str(src).expect("failed to parse enum")
    }

    fn struct_attrs(src: &str) -> Vec<Attribute> {
        let item: syn::ItemStruct = parse_str(src).unwrap();
        item.attrs
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

    fn find_method<'a>(impl_block: &'a syn::ItemImpl, name: &str) -> Option<&'a syn::ImplItemFn> {
        impl_block.items.iter().find_map(|item| {
            let ImplItem::Fn(method) = item else {
                return None;
            };
            (method.sig.ident == name).then_some(method)
        })
    }

    fn find_assoc_type<'a>(
        impl_block: &'a syn::ItemImpl,
        name: &str,
    ) -> Option<&'a syn::ImplItemType> {
        impl_block.items.iter().find_map(|item| {
            let ImplItem::Type(t) = item else { return None };
            (t.ident == name).then_some(t)
        })
    }

    fn find_const<'a>(file: &'a File, ident: &str) -> Option<&'a syn::ItemConst> {
        file.items.iter().find_map(|item| {
            let Item::Const(c) = item else { return None };
            (c.ident == ident).then_some(c)
        })
    }

    fn match_arm_count(method: &syn::ImplItemFn) -> Option<usize> {
        method.block.stmts.iter().find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Match(m), _) => Some(m.arms.len()),
            _ => None,
        })
    }

    fn has_compile_error(ts: TokenStream) -> bool {
        parse2::<File>(ts).is_ok_and(|file| {
            file.items.iter().any(|item| {
                let Item::Macro(m) = item else { return false };
                m.mac
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == "compile_error")
            })
        })
    }

    fn assoc_type_last_segment(impl_block: &syn::ItemImpl, name: &str) -> Option<String> {
        let t = find_assoc_type(impl_block, name)?;
        let syn::Type::Path(tp) = &t.ty else {
            return None;
        };
        tp.path.segments.last().map(|s| s.ident.to_string())
    }

    #[test]
    fn find_attribute_not_found() {
        let attrs = struct_attrs("#[other] struct Foo;");
        assert!(find_attribute("target", &attrs).is_none());
    }

    #[test]
    fn find_attribute_found() {
        let attrs = struct_attrs("#[target(x = 1)] struct Foo;");
        assert!(find_attribute("target", &attrs).is_some());
    }

    #[test]
    fn find_attribute_returns_first_of_duplicates() {
        let attrs = struct_attrs("#[target(a = 1)] #[target(b = 2)] struct Foo;");
        let attr = find_attribute("target", &attrs).unwrap();
        let prefs = attr
            .parse_args_with(Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated)
            .unwrap();
        assert!(prefs.iter().any(|m| m.path.is_ident("a")));
        assert!(!prefs.iter().any(|m| m.path.is_ident("b")));
    }

    #[test]
    fn parse_comma_separated_types_single() {
        let attrs = struct_attrs("#[types(i32)] struct Foo;");
        let attr = find_attribute("types", &attrs).unwrap();
        let types = parse_comma_separated_types(attr).unwrap();
        assert_eq!(types.len(), 1);
        assert!(types[0].path.is_ident("i32"));
    }

    #[test]
    fn parse_comma_separated_types_multiple() {
        let attrs = struct_attrs("#[types(i32, String, bool)] struct Foo;");
        let attr = find_attribute("types", &attrs).unwrap();
        let types = parse_comma_separated_types(attr).unwrap();
        assert_eq!(types.len(), 3);
        assert!(types[0].path.is_ident("i32"));
        assert!(types[1].path.is_ident("String"));
        assert!(types[2].path.is_ident("bool"));
    }

    #[test]
    fn parse_comma_separated_types_empty() {
        let attrs = struct_attrs("#[types()] struct Foo;");
        let attr = find_attribute("types", &attrs).unwrap();
        let types = parse_comma_separated_types(attr).unwrap();
        assert!(types.is_empty());
    }

    #[test]
    fn enum_item_all_derive_generates_impl_and_const() {
        let input = parse_enum("enum Color { Red, Green, Blue }");
        let file = parse_output(enum_item_all_derive(&input));

        let c = find_const(&file, "_COLOR_ITEMS").expect("_COLOR_ITEMS const not found");
        let syn::Type::Array(arr_ty) = c.ty.as_ref() else {
            panic!("expected array type")
        };
        assert!(matches!(arr_ty.elem.as_ref(), syn::Type::Path(tp) if tp.path.is_ident("Color")));
        let syn::Expr::Lit(len_lit) = &arr_ty.len else {
            panic!("expected literal len")
        };
        let syn::Lit::Int(len_int) = &len_lit.lit else {
            panic!("expected int literal")
        };
        assert_eq!(len_int.base10_parse::<usize>().unwrap(), 3);

        let syn::Expr::Array(arr_val) = c.expr.as_ref() else {
            panic!("expected array expr")
        };
        assert_eq!(arr_val.elems.len(), 3);

        let impl_block =
            find_impl(&file, "ItemAll", "Color").expect("impl ItemAll for Color not found");
        assert!(find_method(impl_block, "all").is_some());
    }

    #[test]
    fn enum_item_all_derive_single_variant() {
        let input = parse_enum("enum Single { Only }");
        let file = parse_output(enum_item_all_derive(&input));

        let c = find_const(&file, "_SINGLE_ITEMS").expect("_SINGLE_ITEMS const not found");
        let syn::Type::Array(arr_ty) = c.ty.as_ref() else {
            panic!("expected array type")
        };
        let syn::Expr::Lit(len_lit) = &arr_ty.len else {
            panic!("expected literal len")
        };
        let syn::Lit::Int(len_int) = &len_lit.lit else {
            panic!("expected int literal")
        };
        assert_eq!(len_int.base10_parse::<usize>().unwrap(), 1);

        assert!(find_impl(&file, "ItemAll", "Single").is_some());
    }

    #[test]
    fn enum_item_from_index_derive_generates_impl() {
        let input = parse_enum("enum Dir { In, Out }");
        let file = parse_output(enum_item_from_index_derive(&input));
        let impl_block =
            find_impl(&file, "ItemFromIndex", "Dir").expect("impl ItemFromIndex for Dir not found");
        let method = find_method(impl_block, "from_index").expect("fn from_index not found");
        assert_eq!(match_arm_count(method).expect("no match expr"), 3); // 2 variants + wildcard
    }

    #[test]
    fn enum_item_index_derive_generates_impl() {
        let input = parse_enum("enum Dir { In, Out }");
        let file = parse_output(enum_item_index_derive(&input));
        let impl_block =
            find_impl(&file, "ItemIndex", "Dir").expect("impl ItemIndex for Dir not found");
        let method = find_method(impl_block, "index").expect("fn index not found");
        assert_eq!(match_arm_count(method).expect("no match expr"), 2); // 2 variants, no wildcard
    }

    #[test]
    fn enum_item_as_str_derive_generates_impl() {
        let input = parse_enum("enum Color { Red, Green }");
        let file = parse_output(enum_item_as_str_derive(&input));
        let impl_block =
            find_impl(&file, "ItemAsStr", "Color").expect("impl ItemAsStr for Color not found");
        let method = find_method(impl_block, "as_str").expect("fn as_str not found");
        assert_eq!(match_arm_count(method).expect("no match expr"), 2);

        let syn::ReturnType::Type(_, ret_ty) = &method.sig.output else {
            panic!("expected return type")
        };
        assert!(matches!(ret_ty.as_ref(), syn::Type::Reference(_)));
    }

    #[test]
    fn enum_item_from_str_derive_generates_impl() {
        let input = parse_enum("enum Color { Red, Green }");
        let file = parse_output(enum_item_from_str_derive(&input));

        let impl_block =
            find_impl(&file, "FromStr", "Color").expect("impl FromStr for Color not found");
        assert_eq!(
            assoc_type_last_segment(impl_block, "Err").as_deref(),
            Some("Error")
        );
        let method = find_method(impl_block, "from_str").expect("fn from_str not found");
        assert_eq!(match_arm_count(method).expect("no match expr"), 3); // 2 variants + wildcard

        assert!(find_impl(&file, "ItemFromStr", "Color").is_some());
    }

    #[test]
    fn property_type_derive_no_qty_valid() {
        let input = parse_enum(r#"enum E { #[property(typ = Int)] A, #[property(typ = Bool)] B }"#);
        let file = parse_output(item_kind_property_type_derive(&input, false));
        let impl_block = find_impl(&file, "ItemKindPropertyType", "E").expect("impl not found");

        assert_eq!(
            assoc_type_last_segment(impl_block, "PropertyType").as_deref(),
            Some("PropertyType")
        );
        assert_eq!(
            assoc_type_last_segment(impl_block, "QuantityType").as_deref(),
            Some("QuantityType")
        );

        let prop_method =
            find_method(impl_block, "property_type").expect("fn property_type not found");
        assert_eq!(match_arm_count(prop_method).expect("no match expr"), 2);

        let qty_method =
            find_method(impl_block, "property_quantity").expect("fn property_quantity not found");
        assert_eq!(match_arm_count(qty_method).expect("no match expr"), 2);

        let Stmt::Expr(Expr::Match(m), _) = &qty_method.block.stmts[0] else {
            panic!("expected match")
        };
        for arm in &m.arms {
            let Expr::Path(p) = arm.body.as_ref() else {
                panic!("expected path expr in qty arm")
            };
            assert_eq!(p.path.segments.last().unwrap().ident, "One");
        }
    }

    #[test]
    fn property_type_derive_no_qty_ignores_quantity_key() {
        let input = parse_enum(r#"enum E { #[property(typ = Int, quantity = Multi)] A }"#);
        let ts = item_kind_property_type_derive(&input, false);
        assert!(!has_compile_error(ts.clone()));
        let file = parse_output(ts);
        let impl_block = find_impl(&file, "ItemKindPropertyType", "E").unwrap();
        let qty_method = find_method(impl_block, "property_quantity").unwrap();
        let Stmt::Expr(Expr::Match(m), _) = &qty_method.block.stmts[0] else {
            panic!("expected match")
        };
        let Expr::Path(p) = m.arms[0].body.as_ref() else {
            panic!("expected path")
        };
        assert_eq!(p.path.segments.last().unwrap().ident, "One");
    }

    #[test]
    fn property_type_derive_no_qty_missing_attribute_emits_compile_error() {
        let input = parse_enum("enum E { A, B }");
        assert!(has_compile_error(item_kind_property_type_derive(
            &input, false
        )));
    }

    #[test]
    fn property_type_derive_no_qty_missing_typ_emits_compile_error() {
        let input = parse_enum(r#"enum E { #[property(quantity = One)] A }"#);
        assert!(has_compile_error(item_kind_property_type_derive(
            &input, false
        )));
    }

    #[test]
    fn property_type_derive_with_qty_valid() {
        let input = parse_enum(
            r#"enum E { #[property(typ = Int, quantity = One)] A, #[property(typ = Bool, quantity = Multi)] B }"#,
        );
        let file = parse_output(item_kind_property_type_derive(&input, true));
        let impl_block = find_impl(&file, "ItemKindPropertyType", "E").expect("impl not found");

        let prop_method =
            find_method(impl_block, "property_type").expect("fn property_type not found");
        assert_eq!(match_arm_count(prop_method).expect("no match expr"), 2);

        let qty_method =
            find_method(impl_block, "property_quantity").expect("fn property_quantity not found");
        assert_eq!(match_arm_count(qty_method).expect("no match expr"), 2);

        let Stmt::Expr(Expr::Match(m), _) = &qty_method.block.stmts[0] else {
            panic!("expected match")
        };
        let arm_qtys: Vec<_> = m
            .arms
            .iter()
            .map(|arm| {
                let Expr::Path(p) = arm.body.as_ref() else {
                    panic!("expected path")
                };
                p.path.segments.last().unwrap().ident.to_string()
            })
            .collect();
        assert!(arm_qtys.contains(&"One".to_string()));
        assert!(arm_qtys.contains(&"Multi".to_string()));
    }

    #[test]
    fn property_type_derive_with_qty_missing_attribute_emits_compile_error() {
        let input = parse_enum("enum E { A }");
        assert!(has_compile_error(item_kind_property_type_derive(
            &input, true
        )));
    }

    #[test]
    fn property_type_derive_with_qty_missing_typ_emits_compile_error() {
        let input = parse_enum(r#"enum E { #[property(quantity = One)] A }"#);
        assert!(has_compile_error(item_kind_property_type_derive(
            &input, true
        )));
    }

    #[test]
    fn property_type_derive_with_qty_missing_quantity_emits_compile_error() {
        let input = parse_enum(r#"enum E { #[property(typ = Int)] A }"#);
        assert!(has_compile_error(item_kind_property_type_derive(
            &input, true
        )));
    }
}
