use syn::{Attribute, Error, Ident, ItemEnum, TypePath, punctuated::Punctuated};

use crate::enum_derives::require_attribute;

pub(crate) const SCHEMA_PARAM: &str = "schema";

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
pub(crate) fn to_snake_case(input: &str) -> String {
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
pub(crate) fn method_ident(variant: &Ident) -> Ident {
    let snake = to_snake_case(&variant.to_string());
    if is_rust_keyword(&snake) {
        Ident::new_raw(&snake, variant.span())
    } else {
        Ident::new(&snake, variant.span())
    }
}

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
        Expr, File, ImplItem, ImplItemConst, ImplItemFn, Item, ItemEnum, ItemImpl, Signature, Stmt,
        parse_str, parse2,
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

    pub(crate) fn find_const<'a>(
        impl_block: &'a ItemImpl,
        name: &str,
    ) -> Option<&'a ImplItemConst> {
        impl_block.items.iter().find_map(|item| {
            let ImplItem::Const(constant) = item else {
                return None;
            };
            (constant.ident == name).then_some(constant)
        })
    }

    pub(crate) fn assert_inherited_vis(method: &ImplItemFn) {
        assert!(
            matches!(method.vis, syn::Visibility::Inherited),
            "trait impl method `{}` must not have an explicit visibility qualifier, found {:?}",
            method.sig.ident,
            quote::quote!(#method).to_string()
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_snake_case_basic() {
        assert_eq!(to_snake_case("FullName"), "full_name");
        assert_eq!(to_snake_case("Key"), "key");
        assert_eq!(to_snake_case("Ref"), "ref");
        assert_eq!(to_snake_case("HTTPServer"), "http_server");
        assert_eq!(to_snake_case("File01"), "file01");
    }
}
