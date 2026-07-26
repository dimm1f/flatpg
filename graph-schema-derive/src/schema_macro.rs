use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Error, Ident, Token, TypePath, Visibility,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub(crate) struct SchemaDef {
    vis: Visibility,
    ident: Ident,
    types: Punctuated<TypePath, Token![,]>,
}

impl Parse for SchemaDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vis: Visibility = input.parse()?;
        let ident: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let types = Punctuated::<TypePath, Token![,]>::parse_terminated(input)?;
        Ok(Self { vis, ident, types })
    }
}

fn arity_error(ident: &Ident, count: usize) -> Error {
    Error::new_spanned(
        ident,
        format!(
            "schema!({ident}: ...) expects 3 or 4 types — node kind, edge kind, and property \
             kind enums, plus an optional enum-property registry (found {count}), e.g. \
             schema!({ident}: SimpleNode, SimpleEdge, SimpleProperty) or \
             schema!({ident}: SimpleNode, SimpleEdge, SimpleProperty, SimpleRegistry);"
        ),
    )
}

pub(crate) fn expand(def: SchemaDef) -> TokenStream {
    let SchemaDef { vis, ident, types } = def;
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

    quote! {
        #[derive(Clone, Copy, Default)]
        #vis struct #ident;

        impl flatpg::schema::Schema for #ident {
            type N = #node;
            type E = #edge;
            type P = #property;
            type EPR = #epr;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{find_impl, has_compile_error, parse_output};
    use syn::{ImplItem, Item, ItemStruct};

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
            .map(|p| p.segments.last().unwrap().ident.to_string())
            .collect()
    }

    fn assoc_type<'a>(impl_block: &'a syn::ItemImpl, name: &str) -> &'a syn::Type {
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
        tp.path.segments.last().unwrap().ident.to_string()
    }

    #[test]
    fn parses_three_types_without_registry() {
        let def = parse_schema_def("SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty");
        assert_eq!(def.types.len(), 3);
    }

    #[test]
    fn parses_four_types_with_registry() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty, SimplePropEnumsRegistry",
        );
        assert_eq!(def.types.len(), 4);
    }

    #[test]
    fn parses_optional_pub_visibility() {
        let public = parse_schema_def("pub SimpleSchema: A, B, C");
        let inherited = parse_schema_def("SimpleSchema: A, B, C");
        assert!(matches!(public.vis, Visibility::Public(_)));
        assert!(matches!(inherited.vis, Visibility::Inherited));
    }

    #[test]
    fn missing_colon_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>("SimpleSchema A, B, C").is_err());
    }

    #[test]
    fn too_few_types_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn too_many_types_emits_compile_error() {
        let def = parse_schema_def("SimpleSchema: A, B, C, D, E");
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn expand_generates_unit_struct_with_clone_copy_default() {
        let def = parse_schema_def("SimpleSchema: A, B, C");
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
        let def = parse_schema_def("pub SimpleSchema: A, B, C");
        let file = parse_output(expand(def));
        let item = find_struct(&file, "SimpleSchema");
        assert!(matches!(item.vis, Visibility::Public(_)));
    }

    #[test]
    fn expand_assigns_associated_types() {
        let def = parse_schema_def("SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty");
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(type_name(assoc_type(impl_block, "N")), "SimpleNode");
        assert_eq!(type_name(assoc_type(impl_block, "E")), "SimpleEdge");
        assert_eq!(type_name(assoc_type(impl_block, "P")), "SimpleProperty");
    }

    #[test]
    fn expand_defaults_epr_to_no_enum_props_when_omitted() {
        let def = parse_schema_def("SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty");
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(type_name(assoc_type(impl_block, "EPR")), "NoEnumProps");
    }

    #[test]
    fn expand_uses_provided_epr_when_given() {
        let def = parse_schema_def(
            "SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty, SimpleRegistry",
        );
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(type_name(assoc_type(impl_block, "EPR")), "SimpleRegistry");
    }
}
