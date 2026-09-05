extern crate proc_macro;

mod common;
mod edge_structs_derives;
mod enum_derives;
mod enum_property_registry_derives;
mod enum_property_registry_macro;
mod graph_view_derives;
mod node_structs_derives;
mod property_trait_derives;
mod schema_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Error, ItemEnum, parse_macro_input};

#[proc_macro_derive(ItemAll)]
pub fn enum_item_all(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    enum_derives::enum_item_all_derive(&input).into()
}

#[proc_macro_derive(ItemFromIndex)]
pub fn enum_item_from_index(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    enum_derives::enum_item_from_index_derive(&input).into()
}

#[proc_macro_derive(ItemIndex)]
pub fn enum_item_index(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    enum_derives::enum_item_index_derive(&input).into()
}

#[proc_macro_derive(ItemAsStr)]
pub fn enum_item_as_str(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    enum_derives::enum_item_as_str_derive(&input).into()
}

#[proc_macro_derive(ItemFromStr)]
pub fn enum_item_from_str(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    enum_derives::enum_item_from_str_derive(&input).into()
}

#[proc_macro_derive(NodeItemKind, attributes(properties, node_kind))]
pub fn node_kind_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    let config = match node_structs_derives::parse_node_kind_config(&input) {
        Ok(c) => c,
        Err(e) => return e.into_compile_error().into(),
    };

    let enum_item_all = enum_derives::enum_item_all_derive(&input);
    let enum_item_from_index = enum_derives::enum_item_from_index_derive(&input);
    let enum_item_index = enum_derives::enum_item_index_derive(&input);
    let enum_item_as_str = enum_derives::enum_item_as_str_derive(&input);
    let enum_item_from_str = enum_derives::enum_item_from_str_derive(&input);
    let available_properties =
        node_structs_derives::node_available_properties_derive(&input, &config)
            .unwrap_or_else(Error::into_compile_error);
    let node_structs = node_structs_derives::node_structs_derive(&input, &config)
        .unwrap_or_else(Error::into_compile_error);
    let node_accessor_traits = graph_view_derives::node_accessor_traits_derive(&input, &config)
        .unwrap_or_else(Error::into_compile_error);

    let schema_ty = config.schema;
    quote! {
        #enum_item_all
        #enum_item_from_index
        #enum_item_index
        #enum_item_as_str
        #enum_item_from_str
        #available_properties
        #node_structs
        #node_accessor_traits

        impl #impl_generics ::flatpg::prelude::NodeItemKind<::flatpg::schema::PropKind<#schema_ty>>
            for #ident #ty_generics #where_clause {}
    }
    .into()
}

#[proc_macro_derive(EdgeItemKind, attributes(edge_kind, property))]
pub fn edge_kind_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    let config = match edge_structs_derives::parse_edge_kind_config(&input) {
        Ok(c) => c,
        Err(e) => return e.into_compile_error().into(),
    };

    let enum_item_all = enum_derives::enum_item_all_derive(&input);
    let enum_item_from_index = enum_derives::enum_item_from_index_derive(&input);
    let enum_item_index = enum_derives::enum_item_index_derive(&input);
    let enum_item_as_str = enum_derives::enum_item_as_str_derive(&input);
    let enum_item_from_str = enum_derives::enum_item_from_str_derive(&input);
    let enum_item_property_type = enum_derives::item_kind_property_type_derive(&input, false);
    let edge_structs = edge_structs_derives::edge_structs_derive(&input, &config)
        .unwrap_or_else(Error::into_compile_error);
    let edges_accessor = graph_view_derives::edges_accessor_trait_derive(&input, &config);

    quote! {
        #enum_item_all
        #enum_item_from_index
        #enum_item_index
        #enum_item_as_str
        #enum_item_from_str
        #enum_item_property_type
        #edge_structs
        #edges_accessor

        impl #impl_generics ::flatpg::prelude::EdgeItemKind for #name #ty_generics #where_clause {}
    }
    .into()
}

#[proc_macro_derive(EnumProperty)]
pub fn enum_property_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    let enum_item_all = enum_derives::enum_item_all_derive(&input);
    let enum_item_from_index = enum_derives::enum_item_from_index_derive(&input);
    let enum_item_index = enum_derives::enum_item_index_derive(&input);
    let enum_item_as_str = enum_derives::enum_item_as_str_derive(&input);
    let enum_item_from_str = enum_derives::enum_item_from_str_derive(&input);

    quote! {
        #enum_item_all
        #enum_item_from_index
        #enum_item_index
        #enum_item_as_str
        #enum_item_from_str

        impl #impl_generics ::flatpg::prelude::EnumProperty for #ident #ty_generics #where_clause {}
    }
    .into()
}

#[proc_macro_derive(EnumPropertyRegistry, attributes(enum_type))]
pub fn enum_property_registry_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);

    let enum_item_all = enum_derives::enum_item_all_derive(&input);
    let enum_item_from_index = enum_derives::enum_item_from_index_derive(&input);
    let enum_item_index = enum_derives::enum_item_index_derive(&input);
    let enum_item_as_str = enum_derives::enum_item_as_str_derive(&input);
    let enum_item_from_str = enum_derives::enum_item_from_str_derive(&input);
    let enum_property_registry_impls =
        enum_property_registry_derives::enum_property_registry_derive(&input);

    quote! {
        #enum_item_all
        #enum_item_from_index
        #enum_item_index
        #enum_item_as_str
        #enum_item_from_str
        #enum_property_registry_impls
    }
    .into()
}

#[proc_macro]
pub fn enum_property_registry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as enum_property_registry_macro::RegistryDef);
    enum_property_registry_macro::expand(input).into()
}

#[proc_macro]
pub fn schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as schema_macro::SchemaDef);
    schema_macro::expand(input).into()
}

#[proc_macro_derive(PropertyItemKind, attributes(property_kind, property))]
pub fn property_kind_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    let enum_item_all = enum_derives::enum_item_all_derive(&input);
    let enum_item_from_index = enum_derives::enum_item_from_index_derive(&input);
    let enum_item_index = enum_derives::enum_item_index_derive(&input);
    let enum_item_as_str = enum_derives::enum_item_as_str_derive(&input);
    let enum_item_from_str = enum_derives::enum_item_from_str_derive(&input);
    let enum_item_property_type = enum_derives::item_kind_property_type_derive(&input, true);
    let property_traits = property_trait_derives::property_traits_derive(&input);

    quote! {
        #enum_item_all
        #enum_item_from_index
        #enum_item_index
        #enum_item_as_str
        #enum_item_from_str
        #enum_item_property_type
        #property_traits

        impl #impl_generics ::flatpg::prelude::PropertyItemKind for #name #ty_generics #where_clause {}
    }
    .into()
}
