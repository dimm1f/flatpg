use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Error, Ident, LitStr, Token, TypePath, Visibility,
    parse::{Parse, ParseStream},
};

const KEYS: &str = "`name`, `node_kind`, `edge_kind`, `prop_kind`, `enum_prop_registry`, \
                    `version`";

const USAGE: &str = "schema!(name = SimpleSchema, node_kind = SimpleNode, \
                     edge_kind = SimpleEdge, prop_kind = SimpleProperty, version = \"1.0.0\")";

pub(crate) struct SchemaDef {
    vis: Visibility,
    name: Ident,
    node_kind: TypePath,
    edge_kind: TypePath,
    prop_kind: TypePath,
    enum_prop_registry: Option<TypePath>,
    version: LitStr,
}

impl Parse for SchemaDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let span = input.span();
        let vis: Visibility = input.parse()?;

        let mut name: Option<Ident> = None;
        let mut node_kind: Option<TypePath> = None;
        let mut edge_kind: Option<TypePath> = None;
        let mut prop_kind: Option<TypePath> = None;
        let mut enum_prop_registry: Option<TypePath> = None;
        let mut version: Option<LitStr> = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "name" => parse_and_set(&mut name, &key, input)?,
                "node_kind" => parse_and_set(&mut node_kind, &key, input)?,
                "edge_kind" => parse_and_set(&mut edge_kind, &key, input)?,
                "prop_kind" => parse_and_set(&mut prop_kind, &key, input)?,
                "enum_prop_registry" => parse_and_set(&mut enum_prop_registry, &key, input)?,
                "version" => parse_and_set(&mut version, &key, input)?,
                _ => return Err(unknown_key_error(&key)),
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        match (name, node_kind, edge_kind, prop_kind, version) {
            (Some(name), Some(node_kind), Some(edge_kind), Some(prop_kind), Some(version)) => {
                Ok(Self {
                    vis,
                    name,
                    node_kind,
                    edge_kind,
                    prop_kind,
                    enum_prop_registry,
                    version,
                })
            }
            (name, node_kind, edge_kind, prop_kind, version) => Err(missing_keys_error(
                span,
                &[
                    ("name", name.is_none()),
                    ("node_kind", node_kind.is_none()),
                    ("edge_kind", edge_kind.is_none()),
                    ("prop_kind", prop_kind.is_none()),
                    ("version", version.is_none()),
                ],
            )),
        }
    }
}

fn parse_and_set<T: Parse>(
    slot: &mut Option<T>,
    key: &Ident,
    input: ParseStream,
) -> syn::Result<()> {
    if slot.is_some() {
        return Err(duplicate_key_error(key));
    }
    *slot = Some(input.parse()?);
    Ok(())
}

fn unknown_key_error(key: &Ident) -> Error {
    Error::new_spanned(key, format!("unknown key `{key}`, expected one of {KEYS}"))
}

fn duplicate_key_error(key: &Ident) -> Error {
    Error::new_spanned(key, format!("key `{key}` specified more than once"))
}

/// Builds the error for a call that omitted required keys, from `(key, is_missing)` pairs.
fn missing_keys_error(span: Span, keys: &[(&str, bool)]) -> Error {
    let missing: Vec<&str> = keys
        .iter()
        .filter(|&&(_, is_missing)| is_missing)
        .map(|&(key, _)| key)
        .collect();
    let noun = if missing.len() == 1 { "key" } else { "keys" };
    Error::new(
        span,
        format!(
            "missing {noun} {} in `schema!`\nhelp: {USAGE}",
            quoted_list(&missing)
        ),
    )
}

fn quoted_list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

fn parse_version(lit: &LitStr) -> syn::Result<(u32, u32, u32)> {
    let value = lit.value();
    let parts: Vec<&str> = value.split('.').collect();
    let [major, minor, patch] = parts.as_slice() else {
        return Err(Error::new_spanned(
            lit,
            format!("invalid version `{value}`, expected `major.minor.patch`"),
        ));
    };
    let parse_component = |s: &str| {
        s.parse::<u32>().map_err(|_| {
            Error::new_spanned(
                lit,
                format!(
                    "invalid version `{value}`, expected an unsigned integer component, \
                     found `{s}`"
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
        name,
        node_kind,
        edge_kind,
        prop_kind,
        enum_prop_registry,
        version,
    } = def;

    let epr = match enum_prop_registry {
        Some(registry) => quote!(#registry),
        None => quote!(::flatpg::enum_property::NoEnumProps),
    };

    let (major, minor, patch) = match parse_version(&version) {
        Ok(parts) => parts,
        Err(err) => return err.to_compile_error(),
    };

    let name_str = name.to_string();

    quote! {
        #[derive(
            ::core::clone::Clone,
            ::core::marker::Copy,
            ::core::default::Default,
            ::core::fmt::Debug,
        )]
        #vis struct #name;

        impl ::flatpg::schema::Schema for #name {
            type N = #node_kind;
            type E = #edge_kind;
            type P = #prop_kind;
            type EPR = #epr;

            const NAME: &'static str = #name_str;

            const VERSION: ::flatpg::schema::Version =
                ::flatpg::schema::Version::new(#major, #minor, #patch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{find_const, find_impl, has_compile_error, parse_output};
    use syn::{Expr, ExprLit, ImplItem, Item, ItemStruct, Lit, punctuated::Punctuated};

    const MINIMAL: &str = "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, \
                           version = \"1.0.0\"";

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

    fn name_const_value(impl_block: &syn::ItemImpl) -> String {
        let constant = find_const(impl_block, "NAME").expect("expected a NAME const");
        let Expr::Lit(ExprLit {
            lit: Lit::Str(lit_str),
            ..
        }) = &constant.expr
        else {
            panic!("expected NAME to be initialized with a string literal");
        };
        lit_str.value()
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
    fn parses_required_keys_without_registry() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = SimpleNode, edge_kind = SimpleEdge, \
             prop_kind = SimpleProperty, version = \"1.0.0\"",
        );
        assert_eq!(def.name, "SimpleSchema");
        assert!(def.enum_prop_registry.is_none());
    }

    #[test]
    fn parses_optional_enum_prop_registry() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = SimpleNode, edge_kind = SimpleEdge, \
             prop_kind = SimpleProperty, enum_prop_registry = SimpleRegistry, \
             version = \"1.0.0\"",
        );
        let registry = def.enum_prop_registry.expect("expected a registry");
        assert_eq!(type_name(&syn::Type::Path(registry)), "SimpleRegistry");
    }

    #[test]
    fn parses_keys_in_any_order() {
        let def = parse_schema_def(
            "version = \"1.2.3\", prop_kind = C, name = SimpleSchema, edge_kind = B, \
             node_kind = A",
        );
        assert_eq!(def.name, "SimpleSchema");
        assert_eq!(def.version.value(), "1.2.3");
    }

    #[test]
    fn parses_trailing_comma() {
        let def = parse_schema_def(&format!("{MINIMAL},"));
        assert_eq!(def.name, "SimpleSchema");
    }

    #[test]
    fn parses_path_qualified_kind_types() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = kinds::SimpleNode, edge_kind = kinds::SimpleEdge, \
             prop_kind = kinds::SimpleProperty, version = \"1.0.0\"",
        );
        assert_eq!(type_name(&syn::Type::Path(def.node_kind)), "SimpleNode");
    }

    #[test]
    fn parses_optional_pub_visibility() {
        let public = parse_schema_def(&format!("pub {MINIMAL}"));
        let inherited = parse_schema_def(MINIMAL);
        assert!(matches!(public.vis, Visibility::Public(_)));
        assert!(matches!(inherited.vis, Visibility::Inherited));
    }

    #[test]
    fn parses_restricted_pub_crate_visibility() {
        let def = parse_schema_def(&format!("pub(crate) {MINIMAL}"));
        assert!(matches!(def.vis, Visibility::Restricted(_)));
    }

    #[test]
    fn parses_version_value() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, \
             version = \"1.2.3\"",
        );
        assert_eq!(def.version.value(), "1.2.3");
    }

    #[test]
    fn missing_equals_sign_is_a_parse_error() {
        assert!(
            syn::parse_str::<SchemaDef>(
                "name SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, \
                 version = \"1.0.0\""
            )
            .is_err()
        );
    }

    #[test]
    fn missing_comma_between_keys_is_a_parse_error() {
        assert!(
            syn::parse_str::<SchemaDef>(
                "name = SimpleSchema node_kind = A, edge_kind = B, prop_kind = C, \
                 version = \"1.0.0\""
            )
            .is_err()
        );
    }

    #[test]
    fn unknown_key_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>(&format!("{MINIMAL}, registry = R")).is_err());
    }

    #[test]
    fn duplicate_key_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>(&format!("{MINIMAL}, node_kind = D")).is_err());
    }

    #[test]
    fn duplicate_key_wins_over_an_unparsable_value() {
        let Err(err) = syn::parse_str::<SchemaDef>(&format!("{MINIMAL}, node_kind = ,")) else {
            panic!("expected a parse error");
        };
        assert!(
            err.to_string()
                .contains("`node_kind` specified more than once"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn missing_name_is_a_parse_error() {
        assert!(
            syn::parse_str::<SchemaDef>(
                "node_kind = A, edge_kind = B, prop_kind = C, version = \"1.0.0\""
            )
            .is_err()
        );
    }

    #[test]
    fn missing_kind_key_is_a_parse_error() {
        assert!(
            syn::parse_str::<SchemaDef>(
                "name = SimpleSchema, node_kind = A, prop_kind = C, version = \"1.0.0\""
            )
            .is_err()
        );
    }

    #[test]
    fn missing_version_is_a_parse_error() {
        assert!(
            syn::parse_str::<SchemaDef>(
                "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C"
            )
            .is_err()
        );
    }

    #[test]
    fn empty_input_is_a_parse_error() {
        assert!(syn::parse_str::<SchemaDef>("").is_err());
    }

    #[test]
    fn too_few_version_components_emits_compile_error() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, version = \"1.0\"",
        );
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn empty_version_string_emits_compile_error() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, version = \"\"",
        );
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn non_numeric_version_component_emits_compile_error() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, version = \"1.x.0\"",
        );
        assert!(has_compile_error(expand(def)));
    }

    #[test]
    fn expand_generates_unit_struct_with_clone_copy_default() {
        let def = parse_schema_def(MINIMAL);
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
        let def = parse_schema_def(&format!("pub {MINIMAL}"));
        let file = parse_output(expand(def));
        let item = find_struct(&file, "SimpleSchema");
        assert!(matches!(item.vis, Visibility::Public(_)));
    }

    #[test]
    fn expand_preserves_restricted_visibility_on_struct() {
        let def = parse_schema_def(&format!("pub(crate) {MINIMAL}"));
        let file = parse_output(expand(def));
        let item = find_struct(&file, "SimpleSchema");
        assert!(matches!(item.vis, Visibility::Restricted(_)));
    }

    #[test]
    fn expand_assigns_associated_types() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = SimpleNode, edge_kind = SimpleEdge, \
             prop_kind = SimpleProperty, version = \"1.0.0\"",
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
        let def = parse_schema_def(MINIMAL);
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(type_name(associated_type(impl_block, "EPR")), "NoEnumProps");
    }

    #[test]
    fn expand_uses_provided_epr_when_given() {
        let def = parse_schema_def(&format!("{MINIMAL}, enum_prop_registry = SimpleRegistry"));
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(
            type_name(associated_type(impl_block, "EPR")),
            "SimpleRegistry"
        );
    }

    #[test]
    fn expand_assigns_name_const_from_the_name_key() {
        let def = parse_schema_def(MINIMAL);
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(name_const_value(impl_block), "SimpleSchema");
    }

    #[test]
    fn expand_assigns_version_const() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, \
             version = \"2.5.13\"",
        );
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(version_const_value(impl_block), (2, 5, 13));
    }

    #[test]
    fn expand_accepts_version_with_leading_zeros() {
        let def = parse_schema_def(
            "name = SimpleSchema, node_kind = A, edge_kind = B, prop_kind = C, \
             version = \"01.0.0\"",
        );
        let file = parse_output(expand(def));
        let impl_block = find_impl(&file, "Schema", "SimpleSchema").expect("expected Schema impl");

        assert_eq!(version_const_value(impl_block), (1, 0, 0));
    }
}
