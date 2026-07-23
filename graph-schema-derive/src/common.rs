use syn::{Attribute, Error, ItemEnum, TypePath, punctuated::Punctuated};

use crate::enum_derives::require_attribute;

pub(crate) const SCHEMA_PARAM: &str = "schema";

pub(crate) fn parse_kind_attr<'a>(
    attr_name: &str,
    input: &'a ItemEnum,
    usage: &str,
) -> Result<
    (
        &'a Attribute,
        Punctuated<syn::MetaNameValue, syn::Token![,]>,
    ),
    Error,
> {
    let attr = require_attribute(attr_name, &input.attrs, &input.ident, "enum", usage)?;
    let args =
        attr.parse_args_with(Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated)?;
    Ok((attr, args))
}

pub(crate) fn typ_last_segment_name(typ: &TypePath) -> Result<String, Error> {
    typ.path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .ok_or_else(|| Error::new_spanned(typ, "expected a non-empty `typ` path"))
}

#[cfg(test)]
pub(crate) mod test_support {
    use proc_macro2::TokenStream;
    use syn::{
        Expr, File, ImplItem, ImplItemFn, Item, ItemEnum, ItemImpl, Signature, Stmt, parse_str,
        parse2,
    };

    pub(crate) fn parse_enum(src: &str) -> ItemEnum {
        parse_str(src).expect("failed to parse enum")
    }

    pub(crate) fn parse_output(ts: TokenStream) -> File {
        parse2(ts).expect("generated output is not valid Rust")
    }

    pub(crate) fn find_impl<'a>(
        file: &'a File,
        trait_name: &str,
        self_type: &str,
    ) -> Option<&'a ItemImpl> {
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

    pub(crate) fn find_method<'a>(impl_block: &'a ItemImpl, name: &str) -> Option<&'a ImplItemFn> {
        impl_block.items.iter().find_map(|item| {
            let ImplItem::Fn(method) = item else {
                return None;
            };
            (method.sig.ident == name).then_some(method)
        })
    }

    pub(crate) fn match_arm_count(method: &ImplItemFn) -> Option<usize> {
        method.block.stmts.iter().find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Match(m), _) => Some(m.arms.len()),
            _ => None,
        })
    }

    pub(crate) fn has_compile_error(ts: TokenStream) -> bool {
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

    pub(crate) fn return_type_string(sig: &Signature) -> String {
        let syn::ReturnType::Type(_, ty) = &sig.output else {
            panic!("expected a return type")
        };
        quote::quote!(#ty).to_string()
    }
}
