use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Error, Ident, Token, TypePath, Visibility,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::common::typ_last_segment_name;

pub(crate) struct RegistryDef {
    vis: Visibility,
    ident: Ident,
    enum_types: Punctuated<TypePath, Token![,]>,
}

impl Parse for RegistryDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vis: Visibility = input.parse()?;
        let ident: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let enum_types = Punctuated::<TypePath, Token![,]>::parse_terminated(input)?;
        Ok(Self {
            vis,
            ident,
            enum_types,
        })
    }
}

fn empty_registry_error(ident: &Ident) -> Error {
    Error::new_spanned(
        ident,
        format!(
            "enum_property_registry!({ident}: ...) requires at least one domain enum type — \
             list each #[derive(EnumProperty)] enum to register, e.g. \
             enum_property_registry!({ident}: Status);"
        ),
    )
}

fn duplicate_type_error(dup: &TypePath, name: &str) -> Error {
    Error::new_spanned(
        dup,
        format!(
            "duplicate domain enum `{name}` in enum_property_registry! — each domain enum type \
             may be registered at most once"
        ),
    )
}

fn check_no_duplicates(enum_types: &Punctuated<TypePath, Token![,]>) -> Result<(), Error> {
    let mut seen: Vec<String> = Vec::new();
    for ty in enum_types.iter() {
        let name = typ_last_segment_name(ty)?;
        if seen.contains(&name) {
            return Err(duplicate_type_error(ty, &name));
        }
        seen.push(name);
    }
    Ok(())
}

pub(crate) fn expand(def: RegistryDef) -> TokenStream {
    if def.enum_types.is_empty() {
        return empty_registry_error(&def.ident).to_compile_error();
    }
    if let Err(e) = check_no_duplicates(&def.enum_types) {
        return e.to_compile_error();
    }

    let variants: Result<Vec<TokenStream>, Error> = def
        .enum_types
        .iter()
        .map(|ty| {
            let variant = format_ident!("{}", typ_last_segment_name(ty)?);
            Ok(quote! { #[enum_type(#ty)] #variant })
        })
        .collect();
    let variants = match variants {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };

    let RegistryDef { vis, ident, .. } = def;
    quote! {
        #[derive(Debug, Clone, Copy, EnumPropertyRegistry)]
        #vis enum #ident {
            #(#variants,)*
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{has_compile_error, parse_output};
    use syn::{Item, ItemEnum, Variant};

    fn parse_registry_def(src: &str) -> RegistryDef {
        syn::parse_str(src).expect("failed to parse registry def")
    }

    fn find_enum(file: &syn::File) -> &ItemEnum {
        file.items
            .iter()
            .find_map(|item| match item {
                Item::Enum(e) => Some(e),
                _ => None,
            })
            .expect("expected an enum item")
    }

    fn derive_idents(item: &ItemEnum) -> Vec<String> {
        let attr = item
            .attrs
            .iter()
            .find(|a| a.path().is_ident("derive"))
            .expect("expected a #[derive(...)] attribute");
        attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
            .expect("failed to parse derive list")
            .iter()
            .map(|p| p.segments.last().unwrap().ident.to_string())
            .collect()
    }

    fn enum_type_of(variant: &Variant) -> TypePath {
        let attr = variant
            .attrs
            .iter()
            .find(|a| a.path().is_ident("enum_type"))
            .expect("expected an #[enum_type(...)] attribute");
        attr.parse_args::<TypePath>()
            .expect("failed to parse enum_type arg")
    }

    #[test]
    fn parses_multiple_types_with_and_without_trailing_comma() {
        let with_trailing = parse_registry_def("Registry: Status, Color,");
        let without_trailing = parse_registry_def("Registry: Status, Color");
        assert_eq!(with_trailing.enum_types.len(), 2);
        assert_eq!(without_trailing.enum_types.len(), 2);
    }

    #[test]
    fn parses_optional_pub_visibility() {
        let public = parse_registry_def("pub Registry: Status");
        let inherited = parse_registry_def("Registry: Status");
        assert!(matches!(public.vis, Visibility::Public(_)));
        assert!(matches!(inherited.vis, Visibility::Inherited));
    }

    #[test]
    fn parses_restricted_pub_crate_visibility() {
        let def = parse_registry_def("pub(crate) Registry: Status");
        assert!(matches!(def.vis, Visibility::Restricted(_)));
    }

    #[test]
    fn expand_preserves_restricted_visibility() {
        let def = parse_registry_def("pub(crate) Registry: Status");
        let file = parse_output(expand(def));
        let item = find_enum(&file);
        assert!(matches!(item.vis, Visibility::Restricted(_)));
    }

    #[test]
    fn missing_colon_is_a_parse_error() {
        assert!(syn::parse_str::<RegistryDef>("Registry Status").is_err());
    }

    #[test]
    fn expand_emits_derive_and_enum_type_attrs() {
        let def = parse_registry_def("Registry: Status, Color");
        let file = parse_output(expand(def));
        let item = find_enum(&file);

        assert_eq!(item.ident, "Registry");
        assert_eq!(item.variants.len(), 2);

        let derives = derive_idents(item);
        for expected in ["Debug", "Clone", "Copy", "EnumPropertyRegistry"] {
            assert!(
                derives.contains(&expected.to_string()),
                "missing derive: {expected}"
            );
        }

        assert_eq!(item.variants[0].ident, "Status");
        assert_eq!(
            enum_type_of(&item.variants[0])
                .path
                .segments
                .last()
                .unwrap()
                .ident,
            "Status"
        );
        assert_eq!(item.variants[1].ident, "Color");
        assert_eq!(
            enum_type_of(&item.variants[1])
                .path
                .segments
                .last()
                .unwrap()
                .ident,
            "Color"
        );
    }

    #[test]
    fn expand_derives_variant_name_from_last_path_segment() {
        let def = parse_registry_def("Registry: some_mod::Status");
        let file = parse_output(expand(def));
        let item = find_enum(&file);

        assert_eq!(item.variants.len(), 1);
        assert_eq!(item.variants[0].ident, "Status");
    }

    #[test]
    fn expand_preserves_visibility() {
        let def = parse_registry_def("pub Registry: Status");
        let file = parse_output(expand(def));
        let item = find_enum(&file);

        assert!(matches!(item.vis, Visibility::Public(_)));
    }

    #[test]
    fn empty_registry_emits_compile_error() {
        let def = parse_registry_def("Registry:");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn duplicate_type_emits_compile_error() {
        let def = parse_registry_def("Registry: Status, Status");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn duplicate_type_via_different_paths_emits_compile_error() {
        let def = parse_registry_def("Registry: Status, mod_a::Status");
        assert!(has_compile_error(expand(def)));
    }
}
