# flatpg

flatpg - **FLAT** **P**roperty **G**raph - a schema-driven labeled property graph library for Rust, built on compact flat storage for memory-efficient graphs.

## Status

`flatpg` is early-stage (`0.1.x`) and its API may still change between
releases.

## Overview

Node, edge, and property kinds are defined at compile time as plain Rust enums, deriving `NodeItemKind`, `EdgeItemKind`, and `PropertyItemKind` respectively. A `Schema` implementation ties them together: its associated types `N`, `E`, and `P` name the node, edge, and property kind enums it uses.

`Graph<S>` and `GraphDiff<S>` are generic over a `Schema` `S`. The schema determines the flat array layout: the number of node/edge/property kinds it declares fixes the number and offsets of the storage slots, so the layout is derived from the schema rather than being pointer-based.

A `Graph` is updated by applying a diff (`GraphDiff::apply`), which takes the `Graph` by value, mutates its flat storage directly (appending new nodes/edges, flipping deletion flags, overwriting updated properties), and returns it together with a `Vec<NodeId<S>>` mapping each new node's diff-local id to the `NodeId<S>` it was assigned. `Graph` isn't `Clone`, so there's no way to keep the pre-update version around — Rust's ownership rules just guarantee you can never end up holding two out-of-sync copies at once.

A few notable points about the model:

- Each node kind declares which properties it may carry. Each property declares its type and whether it holds one value or many (`quantity = One` / `quantity = Multi`).
- Every edge is stored as a pair of half-edges, one per endpoint. Either endpoint can look up its incident edges (`get_edges`, `get_edges_count`) without scanning the whole graph. Edges are directed (`Direction::In` / `Direction::Out`). An edge may also carry a single property value, visible from either endpoint. The generated `<Variant>Edge` struct exposes it as a typed `property()` accessor; `Graph::get_edge_property` is the lower-level, untyped form it's built on.
- A diff can add nodes and edges, update a node's property (`update_node_property`), or remove nodes and edges (`remove_node`, `remove_edge`). Diffs apply incrementally, on top of the `Graph` produced by the previous one.

## Public API

All items below are exposed directly by the `flatpg` crate. `flatpg::prelude::*` imports the derive macros and core traits. The modules `flatpg::{edge, enum_property, error, graph, node, property, schema, storage, strings_pool}` re-export the remaining `graph-schema` modules.

The snippets below are excerpts adapted from [`examples/simple_graph.rs`](examples/simple_graph.rs) — see that file for the full, runnable program.

### Defining a schema

A schema is a handful of plain enums, described with derive macros, tied together by an `impl Schema`.

#### `PropertyItemKind`

Derive this on an enum that lists every property a node can carry.

```rust
#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, PropertyItemKind)]
enum SimpleProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = NodeId, quantity = One)]
    Ref,
}
```

Each variant needs a `#[property(typ = ..., quantity = One | Multi)]` attribute (see [Properties](#properties) below for the full list of supported `typ`s). The derive generates one same-named accessor trait per variant: here, a `Key` trait with a `key()` method, and a `Values` trait with a `values()` method. `One` quantity returns a bare value (`key() -> Result<&str, Error>`); `Multi` returns a `Vec` (`values() -> Result<Vec<&str>, Error>`). `Count` and `Ref` follow the same `One` pattern, generating `count()` and `r#ref()` methods. These traits are what let a generated node struct expose `.key()`/`.values()`/`.count()`/`.r#ref()` later on.

A variant can also take `rename = ...`, e.g. `#[property(typ = String, quantity = One, rename = Label)]`. This overrides only the string label used by the generated `ItemAsStr`/`ItemFromStr` impls (`.as_str()` and `.parse()`) — the Rust variant name, and the accessor method derived from it, are unaffected. It's useful when the property needs a Rust-identifier-safe variant name (e.g. avoiding a keyword) but a different serialized/display label.

#### `NodeItemKind`

Derive this on an enum that lists every node kind in the graph.

```rust
#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = SimpleSchema, property_kind = SimpleProperty)]
enum SimpleNode {
    #[properties(Key, Values)]
    Alpha,
    #[properties(Count)]
    Beta,
    #[properties(Ref)]
    Gamma,
}
```

`#[node_kind(...)]` names the schema and property-kind enum this node kind belongs to; each variant's `#[properties(...)]` lists which property kinds it may carry. For every variant this generates:

- a wrapper struct (`AlphaNode<'a>`, `BetaNode<'a>`, `GammaNode<'a>`) implementing the property traits it declared
- a builder (`builders::AlphaNodeBuilder`, `builders::BetaNodeBuilder`, `builders::GammaNodeBuilder`) for constructing new nodes of that kind
- an accessor trait named after the variant (`AlphaNodesAccessor` with an `.alpha()` method, `BetaNodesAccessor` with a `.beta()` method, `GammaNodesAccessor` with a `.gamma()` method), blanket-implemented for any `GraphView<S>` — this is what makes `graph.alpha()` work later

It also generates one combined `Node<'a>` enum (`Node::Alpha(AlphaNode)`, `Node::Beta(BetaNode)`, `Node::Gamma(GammaNode)`) so callers can match on a node generically.

#### `EdgeItemKind`

Derive this on an enum that lists every edge kind.

```rust
#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, EdgeItemKind)]
#[edge_kind(schema = SimpleSchema)]
enum SimpleEdge {
    #[property(typ = None)]
    Base,
    #[property(typ = String)]
    Extended,
}
```

`#[edge_kind(schema = ...)]` names the schema; each variant's `#[property(typ = ... | None)]` says whether that edge kind carries a value. This generates:

- a wrapper struct per variant (`BaseEdge<'a>`, `ExtendedEdge<'a>`)
- variants that declare `typ = ...` (not `None`) also get a typed `.property()` accessor on their wrapper struct — here `ExtendedEdge` gets one, `BaseEdge` doesn't, since `Base` declared `typ = None`
- a combined `Edge<'a>` enum
- a single `EdgesAccessor` trait with an `.edges(node, kind, direction)` method, blanket-implemented for any `GraphView<S>`

#### `EnumProperty`

Derive this on a plain domain enum you want to store as an `Enum<T>`-typed property value.

```rust
#[derive(Debug, Clone, Copy, EnumProperty)]
enum Status {
    Active,
    Inactive,
    Banned,
}
```

On its own this just makes `Status` eligible to be stored inside an `Enum<Status>` property; it still needs to be listed in a registry, which is what `EnumPropertyRegistry` (next) is for.

#### `EnumPropertyRegistry` and `enum_property_registry!`

Every `Enum<T>`-typed domain enum a schema uses must be listed in exactly one registry type, so the schema can assign each one a stable index. The `enum_property_registry!` function-like macro is sugar for writing that registry by hand:

```rust
enum_property_registry!(SimplePropEnumsRegistry: Status);

// equivalent to:
#[derive(Debug, Clone, Copy, EnumPropertyRegistry)]
enum SimplePropEnumsRegistry {
    #[enum_type(Status)]
    Status,
}
```

Either form gives `Status` an `enum_property_index()` and a `TryFrom<PropertyValue> for Status` impl. Wire the registry into your schema as `type EPR = SimplePropEnumsRegistry`.

#### `Schema`

The trait that ties the four enums above together.

```rust
impl Schema for SimpleSchema {
    type N = SimpleNode;
    type E = SimpleEdge;
    type P = SimpleProperty;
    type EPR = SimplePropEnumsRegistry;

    const NAME: &'static str = "SimpleSchema";
    const VERSION: Version = Version::new(1, 0, 0);
}
```

`N`, `E`, `P`, and `EPR` are read at compile time to size and lay out the flat storage arrays that `Graph<S>` and `GraphDiff<S>` use internally: the number of kinds each associated type declares fixes the number of storage slots, so the layout is derived from the schema rather than being pointer-based.

`NAME` and `VERSION` identify the schema itself. Neither affects the storage layout; they are metadata for reporting and for comparing one schema against another.

#### `schema!`

Writing the struct and `impl Schema` by hand is only a few lines, but `schema!` is sugar for exactly that:

```rust
schema!(
    name = SimpleSchema,
    node_kind = SimpleNode,
    edge_kind = SimpleEdge,
    prop_kind = SimpleProperty,
    enum_prop_registry = SimplePropEnumsRegistry,
    version = "1.0.0"
);

// equivalent to:
#[derive(Clone, Copy, Default)]
struct SimpleSchema;

impl Schema for SimpleSchema {
    type N = SimpleNode;
    type E = SimpleEdge;
    type P = SimpleProperty;
    type EPR = SimplePropEnumsRegistry;

    const NAME: &'static str = "SimpleSchema";
    const VERSION: Version = Version::new(1, 0, 0);
}
```

Keys may appear in any order. All of them are required except `enum_prop_registry`, which defaults to `enum_property::NoEnumProps` the crate's built-in placeholder registry for schemas that declare no `Enum<T>`-typed properties:

```rust
schema!(
    name = SimpleSchema,
    node_kind = SimpleNode,
    edge_kind = SimpleEdge,
    prop_kind = SimpleProperty,
    version = "1.0.0"
);
// ... is equivalent to writing `type EPR = flatpg::enum_property::NoEnumProps;` above.
```

`name` names the generated struct and, as a string literal, fills in `Schema::NAME`. `version = "major.minor.patch"` is required and fills in `Schema::VERSION`.

The generated struct is private unless you prefix the argument list with a visibility, which is passed through to it:

```rust
schema!(pub name = SimpleSchema, node_kind = SimpleNode, edge_kind = SimpleEdge, prop_kind = SimpleProperty, version = "1.0.0");
```

### Building and applying graphs

#### `GraphDiff`

A `GraphDiff<S>` is a batch of pending mutations, built up with plain method calls and then applied all at once.

```rust
let mut diff = GraphDiff::<SimpleSchema>::default();
let alpha_id = diff.add_node(
    builders::AlphaNodeBuilder::new()
        .add_property(SimpleProperty::Key, "hello".to_string())
        .unwrap()
        .build(),
);
let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
diff.add_edge(alpha_id, beta_id, SimpleEdge::Base, None);

let (graph, node_remapper) = diff.apply(Graph::<SimpleSchema>::new()).expect("apply diff");
```

`add_node` takes a `NewNode<S>` (built via the generated `<Variant>NodeBuilder`) and returns a diff-local id that can be passed to `add_edge` as either endpoint. `add_edge` also accepts an already-committed `RawNodeId`/`NodeId<S>`/`NewNodeId`, so an edge can connect a brand-new node to one already in the graph. `apply` consumes the diff plus a graph — `Graph::new()` for the very first diff, or the `Graph` a previous `apply` returned — and returns the updated `Graph<S>` together with a `Vec<NodeId<S>>` that maps each `add_node` call's diff-local id (its position in the vec) to the `NodeId<S>` it was assigned in the graph; diffs always apply incrementally, on top of whatever the previous one produced. Beyond `add_node`/`add_edge`, `GraphDiff` also has `remove_node(node_ref)`, `remove_edge(edge)`, and `update_node_property(node_ref, prop_kind, value)`.

#### `Graph`

`Graph<S>` is the flat-storage graph produced by applying diffs; it's queried directly.

`nodes_by_kind(kind)` iterates live (non-deleted) nodes of a kind as `NodeId<S>`; `nodes_by_kind_with_deleted(kind)` includes tombstoned ones. `get_edges(node, kind, direction)` returns the matching `EdgeId<S>`s for one endpoint without scanning the whole graph, and `get_edges_count(...)` is the cheaper count-only version. `get_node_property`/`get_edge_property` are the untyped, lower-level lookups that the generated per-property accessors (`.key()`, `.count()`, ...) are built on top of; `resolve_string`/`resolve_property` turn storage-level values back into owned `PropertyValue`s.

#### `GraphView`

`GraphView<S>` is the trait the generated accessor traits (`<Variant>NodesAccessor`, `EdgesAccessor`) are blanket-implemented over:

`Graph<S>` implements it directly, which is why `graph.alpha()` and `graph.edges(...)` work out of the box on a bare `Graph`. Implementing `GraphView` on a custom wrapper type gets the exact same generated accessors for free.

#### `RawGraph`

`RawGraph<S>` is `Graph<S>`'s flat storage with the wrapper stripped off — the same four fields (`node_meta_storage`, `edge_storage`, `property_storage`, `strings`), all `pub`. Reach for it instead of `GraphDiff` when you need direct mutable access to the columnar arrays: bulk loading, deserialization, or other custom batch construction where going through `GraphDiff::apply`'s per-node/per-edge API would be too slow or the wrong shape for your source data.

```rust
let raw: RawGraph<SimpleSchema> = graph.into();
// ... mutate raw.node_meta_storage / raw.edge_storage / raw.property_storage / raw.strings directly ...
let graph: Graph<SimpleSchema> = raw.try_into().expect("still a well-formed graph");
```

`Graph<S> -> RawGraph<S>` (via `From`) is infallible. The reverse (`RawGraph<S> -> Graph<S>`, via `TryFrom`) runs a full integrity check first — offset arrays well-formed and in bounds, storage slot types matching the schema, node/string/enum refs resolvable, and edge halves correctly paired — and returns `Err` on the first violation found. That check is also exposed directly through the `CheckIntegrity<S>` trait, implemented for both `RawGraph<S>` and `Graph<S>`, so it can be called without a conversion (useful in tests or for asserting an already-built graph is still well-formed).

The check runs on one thread by default. The optional `parallel` feature checks storage slots concurrently through [rayon](https://crates.io/crates/rayon), worth about 3x on a 16-core machine for graphs large enough to pay for the threads; smaller graphs keep taking the sequential path, and either path reports the same error.

```toml
flatpg = { version = "0.1", features = ["parallel"] }
```

Known limitation: enum property validation confirms a `RawEnumId` belongs to *some* registered enum with an in-range variant, not that it belongs to the *specific* enum a given property or edge slot declares. Half-edge pairing validates that mirrored halves exist in matching numbers, not that their property values agree with each other.

### Identifiers and references

#### `NodeId` / `EdgeId`

Typed, schema-resolved identifiers for nodes and edges already committed to a `Graph`.

`NodeId<S>` exposes `.kind()` and `.seq()`; `EdgeId<S>` additionally exposes `.src_node()`, `.dst_node()`, and `.direction()`. Both know their schema, so `.kind()` returns the actual `SimpleNode`/`SimpleEdge` variant rather than a raw index.

#### `RawNodeId` / `RawEdgeId` / `EdgeHandle`

Untyped, schema-erased counterparts to the typed ids above.

- `RawNodeId` is what lets a diff reference a node before its typed id is known: a new node earlier in the same diff, or, as above, a `NodeId`-typed property pointing at a node committed by an earlier, already-applied diff.
- `RawEdgeId` and `EdgeHandle` play the same role for edges internally.

#### `Direction`

Every edge is stored as a pair of half-edges, one per endpoint, so either side can look up its incident edges without scanning the whole graph. `Direction` says which half is being looked at: `Out` is the half stored on the source node (pointing at the destination), `In` is the half stored on the destination node (pointing back at the source).

### Properties

#### `PropertyValue` and `PropertyType`

`PropertyValue` is the value type passed into `add_property`, `update_node_property`, and edge properties. It has one variant per supported type — the same set of `typ`s usable in a `#[property(typ = ...)]` attribute:

`From` impls exist for:

- the corresponding primitives
- `RawNodeId` / `NodeId<S>`
- `String`
- any `#[derive(EnumProperty)]` type registered in the schema's `EPR`

— so `.add_property(SimpleProperty::Count, 42i32)` and `.add_property(SimpleProperty::State, Status::Active)` both just work.

`PropertyType` is the type-only counterpart, used in error messages; a property kind's `Enum<T>` maps to `PropertyType::Enum`. Its variants:

- `Bool`
- `Byte`
- `Short`
- `Int`
- `Long`
- `Float`
- `Double`
- `NodeId`
- `String`
- `Enum`

#### `QuantifiedProperty`

What `update_node_property` takes, so single values and `Vec`s can both be passed without an explicit wrapper:

A bare `PropertyValue` converts into `One`, and a `Vec<PropertyValue>` converts into `Multi`, via `From` — matching the `quantity = One | Multi` a property kind declared.

#### `StoredProperty`

The resolved-in-storage form of a property, as returned by `Graph::get_node_property`/`get_edge_property`. It mirrors `PropertyValue`, except strings stay interned (`RawStringId`) until resolved:

`Graph::resolve_property` turns a `StoredProperty` into an owned `PropertyValue`, resolving any interned string. Note that resolving a `RawStringId` allocates: it clones the interned string into a new owned `String`, so calling this on a `String`-typed property is not free. In practice both types are rarely touched directly: the derives generate typed accessors that do this conversion automatically — `NewNode::add_property(prop_kind, value)` for building nodes, and per-property read methods like `.key()`, `.values()`, `.count()`, `.r#ref()`, `.state()` for reading them back. (`.r#ref()` is a raw identifier, since a `Ref` property collides with the `ref` keyword.)

### Errors

#### `Error` and `Result`

`error::Error` is a `thiserror`-based enum used throughout the crate's fallible APIs — property lookups, edge queries, diff application, and so on.

```rust
match alpha.key() {
    Ok(key) => println!("{key}"),
    Err(Error::PropertyIndexNotFound) => println!("no value set"),
    Err(e) => eprintln!("lookup failed: {e}"),
}
```

Variants cover cases like an invalid property type, an unresolved node/edge/enum reference, a malformed or out-of-bounds offset array, or a missing reverse edge half. Each has a matching constructor function (e.g. `Error::property_not_supported(...)`), which is what the generated accessors and `Graph`/`GraphDiff` methods use internally to build these errors. `error::Result<T>` is simply `Result<T, Error>`.

### Low-level storage

`EdgeStorage<S>` and `PropertyStorage<S>` are each a `Vec` of per-slot structs (`storage::EdgeStorageSlot`, `storage::PropertyStorageSlot`), one slot per `(node_kind, direction, edge_kind)` or `(node_kind, property_kind)` combination, at the index `Schema` computes for it. Each slot bundles its own CSR offsets (`Vec<Offset>`) together with its neighbors (edges) or values (`storage::StorageArray`, a columnar array typed per the schema). `NodeMetaStorage<S>` is a separate wrapper holding per-node metadata. They're public for introspection, but normal usage goes entirely through `Graph`/`GraphDiff` rather than these directly. [`RawGraph`](#rawgraph) is the sanctioned way to get mutable access to them when you do need it.

## Workspace

The crate is organized as a small workspace:

- [`graph-schema`](graph-schema) - the core schema traits, flat storage, and graph types.
- [`graph-schema-derive`](graph-schema-derive) - derive macros for node, edge, and property enums.
- `flatpg` (this crate) - re-exports both, as the entry point for consumers.

## Example

Putting the `SimpleProperty`/`SimpleNode`/`SimpleEdge`/`SimpleSchema` pieces introduced above together with the diff-and-query flow from [Building and applying graphs](#building-and-applying-graphs) gives the complete, runnable program below:

```rust
use flatpg::{
    edge::{Direction, StoredEdge},
    enum_property::NoEnumProps,
    graph::{Graph, builder::GraphDiff},
    prelude::*,
    schema::{Schema, Version},
};

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, PropertyItemKind)]
enum SimpleProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = NodeId, quantity = One)]
    Ref,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = SimpleSchema, property_kind = SimpleProperty)]
enum SimpleNode {
    #[properties(Key, Values)]
    Alpha,
    #[properties(Count)]
    Beta,
    #[properties(Ref)]
    Gamma,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, EdgeItemKind)]
#[edge_kind(schema = SimpleSchema)]
enum SimpleEdge {
    #[property(typ = None)]
    Base,
    #[property(typ = String)]
    Extended,
}

#[derive(Debug, Clone, Copy, Default)]
struct SimpleSchema;

impl Schema for SimpleSchema {
    type N = SimpleNode;
    type E = SimpleEdge;
    type P = SimpleProperty;
    type EPR = NoEnumProps;

    const NAME: &'static str = "SimpleSchema";
    const VERSION: Version = Version::new(1, 0, 0);
}

let mut diff = GraphDiff::<SimpleSchema>::default();
let alpha_id = diff.add_node(
    builders::AlphaNodeBuilder::new()
        .add_property(SimpleProperty::Key, "hello".to_string())
        .unwrap()
        .build(),
);
let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
diff.add_edge(alpha_id, beta_id, SimpleEdge::Base, None);

let (graph, _) = diff.apply(Graph::<SimpleSchema>::new()).expect("apply diff");

let alpha = graph.nodes_by_kind(SimpleNode::Alpha).next().expect("Alpha node");
assert_eq!(AlphaNode::new(&graph, alpha.seq()).key().unwrap(), "hello");
assert_eq!(
    graph.get_edges(alpha, SimpleEdge::Base, Direction::Out).unwrap().len(),
    1
);
```

See [`examples/simple_graph.rs`](examples/simple_graph.rs) for a full, runnable version. It also shows a
cross-diff edge made via `RawNodeId`, and an edge that carries a property. See
[`tests/graph_tests/`](tests/graph_tests/) for more on querying, updating, and removing nodes and edges.
