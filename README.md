# flatpg

flatpg - **FLAT** **P**roperty **G**raph - a schema-driven labeled property graph library for Rust, built on compact flat storage for memory-efficient graphs.

## Overview

Node, edge, and property kinds are defined at compile time as plain Rust enums, deriving `NodeItemKind`, `EdgeItemKind`, and `PropertyItemKind` respectively. A `Schema` implementation ties them together: its associated types `N`, `E`, and `P` name the node, edge, and property kind enums it uses.

`Graph<S>` and `GraphDiff<S>` are generic over a `Schema` `S`. `S` determines the flat array layout: the number of node/edge/property kinds it declares fixes the number and offsets of the storage slots, so the layout is derived from the schema rather than being pointer-based.

A `Graph` is updated by applying a diff (`GraphDiff::apply`), which takes the `Graph` by value, mutates its flat storage directly (appending new nodes/edges, flipping deletion flags, overwriting updated properties), and returns it. `Graph` isn't `Clone`, so there's no way to keep the pre-update version around — Rust's ownership rules just guarantee you can never end up holding two out-of-sync copies at once.

A few notable points about the model:

- Each node kind declares which properties it may carry. Each property declares its type and whether it holds one value or many (`quantity = One` / `quantity = Multi`).
- Every edge is stored as a pair of half-edges, one per endpoint. Either endpoint can look up its incident edges (`get_edges`, `get_edges_count`) without scanning the whole graph. Edges are directed (`Direction::In` / `Direction::Out`). An edge may also carry a single property value, visible from either endpoint. The generated `<Variant>Edge` struct exposes it as a typed `property()` accessor; `Graph::get_edge_property` is the lower-level, untyped form it's built on.
- A node doesn't need to be added to the graph yet to be referenced. Other nodes and edges in the same diff can point at it, e.g. as an edge endpoint or a `NodeRef`-typed property.
- A diff can add nodes and edges, update a node's property (`update_node_property`), or remove nodes and edges (`remove_node`, `remove_edge`). Diffs apply incrementally, on top of the `Graph` produced by the previous one.

## Workspace

The crate is organized as a small workspace:

- [`graph-schema`](graph-schema) - the core schema traits, flat storage, and graph types.
- [`graph-schema-derive`](graph-schema-derive) - derive macros for node, edge, and property enums.
- `flatpg` (this crate) - re-exports both, as the entry point for consumers.

## Example

```rust
use flatpg::{graph::Graph, graph::GraphDiff, node::NodeRef, prelude::*, schema::Schema};
use graph_schema::edge::Direction;

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
}

let mut diff = GraphDiff::<SimpleSchema>::default();
let a_id = diff.add_node(
    builders::ANodeBuilder::new()
        .add_property(SimpleProperty::Key, "hello".to_string())
        .unwrap()
        .build(),
);
let b_id = diff.add_node(builders::BNodeBuilder::new().build());
diff.add_edge(a_id, b_id, SimpleEdge::Base, None);

let graph = diff.apply(Graph::<SimpleSchema>::new()).expect("apply diff");

let a = graph.nodes_by_kind(SimpleNode::A).next().expect("A node");
assert_eq!(ANode::new(&graph, a.seq()).key().unwrap(), "hello");
assert_eq!(
    graph.get_edges(a, SimpleEdge::Base, Direction::Out).unwrap().len(),
    1
);
```

See [`examples/simple_graph.rs`](examples/simple_graph.rs) for a full, runnable version. It also shows a
cross-diff edge made via `NodeRef`, and an edge that carries a property. See
[`tests/graph_tests.rs`](tests/graph_tests.rs) for more on querying, updating, and removing nodes and edges.

## Status

`flatpg` is early-stage (`0.1.x`) and its API may still change between
releases.
