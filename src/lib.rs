//! A schema-driven, flat-storage labeled property graph data structure.
//!
//! Node, edge, and property kinds are defined at compile time via the
//! [`Schema`](schema::Schema) trait and the derive macros re-exported in
//! [`prelude`]. Graphs are built up through [`graph::GraphDiff`] and applied
//! to a [`graph::Graph`], which stores nodes and edges in flat, indexed
//! storage rather than as a pointer-based structure.

pub mod prelude {

    pub use graph_schema_derive::{
        EdgeItemKind, EnumProperty, EnumPropertyRegistry, ItemAll, ItemAsStr, ItemFromIndex,
        ItemFromStr, ItemIndex, NodeItemKind, PropertyItemKind, enum_property_registry,
    };

    pub use graph_schema::{
        AvailableProperties, EdgeDirectionKind, EdgeItemKind, EnumProperty, EnumPropertyIndex,
        EnumPropertyRegistry, ItemAll, ItemAsStr, ItemFromIndex, ItemFromStr, ItemIndex,
        ItemKindPropertyType, NodeItemKind, PropertyItemKind,
    };

    pub use graph_schema::graph::GraphView;
}

pub use graph_schema::{edge, error, graph, node, property, schema, storage};
