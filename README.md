# flatpg

flatpg - **FLAT** **P**roperty **G**raph - a schema-driven labeled property graph library for Rust, built on compact flat storage for memory-efficient graphs.

Node, edge, and property kinds are defined at compile time as plain Rust
enums, using derive macros to generate the boilerplate for indexing,
(de)serialization to/from string, and per-kind property access. Graphs are
built up as a sequence of diffs ([`GraphDiff`]) applied to a [`Graph`], which stores all nodes and edges in flat, indexed storage rather than a
pointer-based structure.

The crate is organized as a small workspace:

- [`graph-schema`](graph-schema) - the core schema traits, flat storage, and graph types.
- [`graph-schema-derive`](graph-schema-derive) - derive macros for node, edge, and property enums.
- `flatpg` (this crate) - re-exports both, as the entry point for consumers.

## Example

```rust
use flatpg::{graph::Graph, graph::GraphDiff, node::NodeRef, prelude::*, schema::Schema};
use graph_schema::edge::BiDirection;

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, PropertyItemKind)]
enum SimpleProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = NodeRef, quantity = One)]
    Ref,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = SimpleSchema, property_kind = SimpleProperty)]
enum SimpleNode {
    #[properties(Key, Values)]
    A,
    #[properties(Count)]
    B,
    #[properties(Ref)]
    C,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, EdgeItemKind)]
enum SimpleEdge {
    #[property(typ = None)]
    Base,
    #[property(typ = String)]
    Extended,
}

#[derive(Clone, Copy, Default)]
struct SimpleSchema;

impl Schema for SimpleSchema {
    type N = SimpleNode;
    type E = SimpleEdge;
    type P = SimpleProperty;
    type D = BiDirection;
}

let mut diff = GraphDiff::<SimpleSchema>::default();
diff.add_node(
    builders::ANodeBuilder::new()
        .add_property(SimpleProperty::Key, "hello".to_string())
        .unwrap()
        .build(),
);

let graph = diff.apply(Graph::<SimpleSchema>::new()).expect("apply diff");
```

See [`examples/simple_graph.rs`](examples/simple_graph.rs) for a full,
runnable version, including edges between nodes via [`NodeRef`].

## Status

`flatpg` is early-stage (`0.1.0`) and its API may still change between
releases.
