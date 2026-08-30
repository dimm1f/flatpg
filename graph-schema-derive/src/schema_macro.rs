use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Error, Ident, LitStr, Token, TypePath, Visibility,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub(crate) struct SchemaDef {
    vis: Visibility,
    ident: Ident,
    types: Punctuated<TypePath, Token![,]>,
    version: LitStr,
}

impl Parse for SchemaDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vis: Visibility = input.parse()?;
        let ident: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let types = Punctuated::<TypePath, Token![,]>::parse_separated_nonempty(input)?;
        input.parse::<Token![;]>()?;
        let version_kw: Ident = input.parse()?;
        if version_kw != "version" {
            return Err(Error::new_spanned(version_kw, "expected `version`"));
        }
        input.parse::<Token![=]>()?;
        let version: LitStr = input.parse()?;
        Ok(Self {
            vis,
            ident,
            types,
            version,
        })
    }
}

fn arity_error(ident: &Ident, count: usize) -> Error {
    Error::new_spanned(
        ident,
        format!(
            "schema!({ident}: ...) expects 3 or 4 types — node kind, edge kind, and property \
             kind enums, plus an optional enum-property registry (found {count}), followed by \
             `; version = \"major.minor.patch\"`, e.g. \
             schema!({ident}: SimpleNode, SimpleEdge, SimpleProperty; version = \"1.0.0\") or \
             schema!({ident}: SimpleNode, SimpleEdge, SimpleProperty, SimpleRegistry; version = \"1.0.0\");"
        ),
    )
}

fn parse_version(lit: &LitStr) -> syn::Result<(u32, u32, u32)> {
    let value = lit.value();
    let parts: Vec<&str> = value.split('.').collect();
    let [major, minor, patch] = parts.as_slice() else {
        return Err(Error::new_spanned(
            lit,
            format!(
                "invalid version \"{value}\": expected exactly 3 dot-separated components, \
                 e.g. \"1.0.0\""
            ),
        ));
    };
    let parse_component = |s: &str| {
        s.parse::<u32>().map_err(|_| {
            Error::new_spanned(
                lit,
                format!(
                    "invalid version component \"{s}\" in \"{value}\": expected an unsigned \
                     integer"
                ),
            )
        })
    };
    Ok((
        parse_component(major)?,
        parse_component(minor)?,
        parse_component(patch)?,
    ))
}

pub(crate) fn expand(def: SchemaDef) -> TokenStream {
    let SchemaDef {
        vis,
        ident,
        types,
        version,
    } = def;
    let types: Vec<TypePath> = types.into_iter().collect();

    let (node, edge, property, epr) = match types.as_slice() {
        [node, edge, property] => (
            node,
            edge,
            property,
            quote!(flatpg::enum_property::NoEnumProps),
        ),
        [node, edge, property, epr] => (node, edge, property, quote!(#epr)),
        other => return arity_error(&ident, other.len()).to_compile_error(),
    };

    let (major, minor, patch) = match parse_version(&version) {
        Ok(parts) => parts,
        Err(err) => return err.to_compile_error(),
    };

    quote! {
        #[derive(Clone, Copy, Default, Debug)]
        #vis struct #ident;

        impl flatpg::schema::Schema for #ident {
            type N = #node;
            type E = #edge;
            type P = #property;
            type EPR = #epr;

            const VERSION: flatpg::schema::Version =
                flatpg::schema::Version::new(#major, #minor, #patch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{find_const, find_impl, has_compile_error, parse_output};
    use syn::{Expr, ExprLit, ImplItem, Item, ItemStruct, Lit};

    fn parse_schema_def(src: &str) -> SchemaDef {
        syn::parse_str(src).expect("failed to parse schema def")
    }

    fn find_struct<'a>(file: &'a syn::File, name: &str) -> &'a ItemStruct {
        file.items
            .iter()
            .find_map(|item| match item {
                Item::Struct(s) if s.ident == name => Some(s),
                _ => None,
            })
            .expect("expected a struct item")
    }

    fn derive_idents(attrs: &[syn::Attribute]) -> Vec<String> {
        attrs
            .iter()
            .find(|a| a.path().is_ident("derive"))
            .expect("expected a #[derive(...)] attribute")
            .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
            .expect("failed to parse derive list")
            .iter()
            .map(|p| {
                p.segments
                    .last()
                    .expect("the path must contain an element")
                    .ident
                    .to_string()
            })
            .collect()
    }

    fn associated_type<'a>(impl_block: &'a syn::ItemImpl, name: &str) -> &'a syn::Type {
        impl_block
            .items
            .iter()
            .find_map(|item| match item {
                ImplItem::Type(t) if t.ident == name => Some(&t.ty),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected associated type `{name}`"))
    }

    fn type_name(ty: &syn::Type) -> String {
        let syn::Type::Path(tp) = ty else {
            panic!("expected a type path")
        };
        tp.path
            .segments
            .last()
            .expect("the path must contain an element")
            .ident
            .to_string()
    }

    fn version_const_value(impl_block: &syn::ItemImpl) -> (u32, u32, u32) {
        let constant = find_const(impl_block, "VERSION").expect("expected a VERSION const");
        let Expr::Call(call) = &constant.expr else {
            panic!("expected VERSION to be initialized with a call expression");
        };
        let args: Vec<u32> = call
            .args
            .iter()
            .map(|arg| {
                let Expr::Lit(ExprLit {
                    lit: Lit::Int(lit_int),
                    ..
                }) = arg
                else {
                    panic!("expected an integer literal argument to Version::new");
                };
                lit_int
                    .base10_parse()
                    .expect("expected a valid u32 literal")
            })
            .collect();
        let [major, minor, patch] = args.as_slice() else {
            panic!("expected exactly 3 arguments to Version::new");
        };
        (*major, *minor, *patch)
    }

    #[test]
    fn parses_three_types_without_registry() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty; version = \"1.0.0\"",
        );
        assert_eq!(def.types.len(), 3);
    }

    #[test]
    fn parses_four_types_with_registry() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty, SimplePropEnumsRegistry; \
             version = \"1.0.0\"",
        );
        assert_eq!(def.types.len(), 4);
    }

    #[test]
    fn parses_optional_pub_visibility() {
        let public = parse_schema_def("pub SimpleSchema: A, B, C; version = \"1.0.0\"");
        let inherited = parse_schema_def("SimpleSchema: A, B, C; version = \"1.0.0\"");
        assert!(matches!(public.vis, Visibility::Public(_)));
        assert!(matches!(inherited.vis, Visibility::Inherited));
    }

    #[test]
    fn parses_restricted_pub_crate_visibility() {
        let def = parse_schema_def("pub(crate) SimpleSchema: A, B, C; version = \"1.0.0\"");
        assert!(matches!(def.vis, Visibility::Restricted(_)));
    }

    #[test]
    fn parses_version_clause() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"1.2.3\"");
        assert_eq!(def.version.value(), "1.2.3");
    }

    #[test]
    fn expand_preserves_restricted_visibility_on_struct() {
        let def = parse_schema_def("pub(crate) SimpleSchema: A, B, C; version = \"1.0.0\"");
        let file = parse_output(expand(def));
        let item = find_struct(&file, "SimpleSchema");
        assert!(matches!(item.vis, Visibility::Restricted(_)));
    }

    #[test]
    fn missing_colon_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>("SimpleSchema A, B, C").is_err());
    }

    #[test]
    fn missing_version_clause_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>("SimpleSchema: A, B, C").is_err());
    }

    #[test]
    fn wrong_version_keyword_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>("SimpleSchema: A, B, C; ver = \"1.0.0\"").is_err());
    }

    #[test]
    fn too_few_types_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B; version = \"1.0.0\"");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn too_many_types_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B, C, D, E; version = \"1.0.0\"");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn too_few_version_components_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"1.0\"");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn empty_version_string_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"\"");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn non_numeric_version_component_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"1.x.0\"");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn expand_generates_unit_struct_with_clone_copy_default() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"1.0.0\"");
        let file = parse_output(expand(def));
        let item = find_struct(&file, "SimpleSchema");

        for expected in ["Clone", "Copy", "Default"] {
            assert!(
                derive_idents(&item.attrs).contains(&expected.to_string()),
                "missing derive: {expected}"
            );
        }
    }

    #[test]
    fn expand_preserves_visibility_on_struct() {
        let def = parse_schema_def("pub SimpleSchema: A, B, C; version = \"1.0.0\"");
        let file = parse_output(expand(def));
        let item = find_struct(&file, "SimpleSchema");
        assert!(matches!(item.vis, Visibility::Public(_)));
    }

    #[test]
    fn expand_assigns_associated_types() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty; version = \"1.0.0\"",
        );
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(type_name(associated_type(impl_block, "N")), "SimpleNode");
        assert_eq!(type_name(associated_type(impl_block, "E")), "SimpleEdge");
        assert_eq!(
            type_name(associated_type(impl_block, "P")),
            "SimpleProperty"
        );
    }

    #[test]
    fn expand_defaults_epr_to_no_enum_props_when_omitted() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty; version = \"1.0.0\"",
        );
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(type_name(associated_type(impl_block, "EPR")), "NoEnumProps");
    }

    #[test]
    fn expand_uses_provided_epr_when_given() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty, SimpleRegistry; \
             version = \"1.0.0\"",
        );
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(
            type_name(associated_type(impl_block, "EPR")),
            "SimpleRegistry"
        );
    }

    #[test]
    fn expand_assigns_version_const() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"2.5.13\"");
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(version_const_value(impl_block), (2, 5, 13));
    }

    #[test]
    fn expand_accepts_version_with_leading_zeros() {
        let def = parse_schema_def("SimpleSchema: A, B, C; version = \"01.0.0\"");
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(version_const_value(impl_block), (1, 0, 0));
    }
}
