//! Reports how many bytes an edge kind's per-edge storage occupies.
//!
//! Counts container lengths rather than sampling RSS, so the numbers are exact and free of
//! allocator slack; RSS is printed alongside only as a sanity check. Arrays indexed by node
//! seq — every slot's `offsets` — are left out: they scale with nodes rather than edges and
//! are unchanged by the shared edge property store, so including them would only dilute the
//! comparison.
//!
//! Run with `cargo run --release --example mem_probe`.

use flatpg::{
    graph::{Graph, builder::GraphDiff, raw::RawGraph},
    prelude::*,
    property::{PropertyType, PropertyValue},
    schema::Schema,
    storage::OffsetStorage,
};
use test_fixtures::{TestEdge, TestSchema, builders};

const NODES: usize = 200_000;
const EDGES_PER_NODE: usize = 4;

/// Bytes one element of a storage array takes.
fn element_size(typ: PropertyType) -> usize {
    match typ {
        PropertyType::None => 0,
        PropertyType::Bool | PropertyType::Byte => 1,
        PropertyType::Short => 2,
        PropertyType::Int | PropertyType::Float | PropertyType::String | PropertyType::Enum => 4,
        PropertyType::Long | PropertyType::Double | PropertyType::NodeId => 8,
    }
}

fn build(kind: TestEdge, value: Option<PropertyValue>) -> Graph<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let ids: Vec<usize> = (0..NODES)
        .map(|_| diff.add_node(builders::AlphaNodeBuilder::new().build()))
        .collect();

    let stride = (NODES / (EDGES_PER_NODE + 1)).max(1);
    for i in 0..NODES {
        for k in 1..=EDGES_PER_NODE {
            diff.add_edge(ids[i], ids[(i + k * stride) % NODES], kind, value.clone());
        }
    }
    diff.apply(Graph::new()).expect("apply diff").0
}

/// Sums the arrays that scale with the number of half-edges, plus the kind's property store.
fn edge_bytes(raw: &RawGraph<TestSchema>, kind: TestEdge) -> (usize, usize) {
    let mut halves = 0usize;
    let mut per_half = 0usize;
    for (node_kind, direction, edge_kind) in TestSchema::edge_storage_slots_iter() {
        if edge_kind != kind {
            continue;
        }
        let slot = &raw.edge_storage
            [TestSchema::edge_storage_slot(node_kind, direction, edge_kind).index()];
        halves += slot.neighbors().len();
        per_half += slot.neighbors().len() * size_of::<flatpg::node::RawNodeId>();
        per_half += slot.edges().len() * size_of::<flatpg::storage::EdgeSeq>();
    }

    let store = &raw.edge_property_storage[kind.index()];
    let store_bytes = store.values().len() * element_size(store.values().typ())
        + store.offsets().len() * size_of::<flatpg::storage::Offset>();

    (halves / 2, per_half + store_bytes)
}

fn rss_kb() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

fn main() {
    let cases = [
        ("Plain     (None)  ", TestEdge::Plain, None),
        (
            "Active    (Bool)  ",
            TestEdge::Active,
            Some(PropertyValue::Bool(true)),
        ),
        (
            "Priority  (Short) ",
            TestEdge::Priority,
            Some(PropertyValue::Short(7)),
        ),
        (
            "Distance  (Int)   ",
            TestEdge::Distance,
            Some(PropertyValue::Int(7)),
        ),
        (
            "Timestamp (Long)  ",
            TestEdge::Timestamp,
            Some(PropertyValue::Long(7)),
        ),
    ];

    println!("{NODES} nodes x {EDGES_PER_NODE} edges");
    println!(
        "{:<20}{:>12}{:>14}{:>12}",
        "kind", "edges", "bytes", "B/edge"
    );
    for (label, kind, value) in cases {
        let before = rss_kb();
        let raw: RawGraph<TestSchema> = build(kind, value).into();
        let (edges, bytes) = edge_bytes(&raw, kind);
        let rss = rss_kb().saturating_sub(before);
        println!(
            "{label:<20}{edges:>12}{bytes:>14}{:>12.2}   (RSS delta {rss} kB)",
            bytes as f64 / edges as f64
        );
        drop(raw);
    }
}
