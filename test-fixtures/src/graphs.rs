//! Graph generators shared by flatpg's benchmarks.

use flatpg::{graph::builder::GraphDiff, node::NewNode, property::PropertyValue};

use crate::{Status, TestEdge, TestProperty, TestSchema, builders};

/// Builds a node whose kind and property set cycle through `Alpha`, `Beta`, and `Gamma`.
pub fn node_for(i: usize) -> NewNode<TestSchema> {
    match i % 3 {
        0 => builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, format!("key-{i}"))
            .unwrap()
            .add_property(TestProperty::Values, format!("v{i}-a"))
            .unwrap()
            .add_property(TestProperty::Values, format!("v{i}-b"))
            .unwrap()
            .add_property(TestProperty::State, Status::Active)
            .unwrap()
            .build(),
        1 => builders::BetaNodeBuilder::new()
            .add_property(TestProperty::Count, i as i32)
            .unwrap()
            .build(),
        _ => builders::GammaNodeBuilder::new()
            .add_property(TestProperty::Flag, i % 2 == 0)
            .unwrap()
            .add_property(TestProperty::Level, (i % 128) as u8)
            .unwrap()
            .add_property(TestProperty::Rank, (i % 1000) as i16)
            .unwrap()
            .add_property(TestProperty::BigCount, i as i64)
            .unwrap()
            .add_property(TestProperty::Ratio, i as f32)
            .unwrap()
            .add_property(TestProperty::Score, i as f64)
            .unwrap()
            .add_property(TestProperty::Tags, Status::Active)
            .unwrap()
            .add_property(TestProperty::Tags, Status::Inactive)
            .unwrap()
            .build(),
    }
}

/// Builds a diff creating `node_count` nodes, each with `edges_per_node` outgoing
/// [`TestEdge::Labeled`] edges to evenly strided destinations.
pub fn build_bulk_diff(node_count: usize, edges_per_node: usize) -> GraphDiff<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let ids: Vec<usize> = (0..node_count)
        .map(|i| diff.add_node(node_for(i)))
        .collect();

    let stride = (node_count / (edges_per_node + 1)).max(1);
    for (i, &src) in ids.iter().enumerate() {
        for k in 1..=edges_per_node {
            let dst = ids[(i + k * stride) % node_count];
            diff.add_edge(
                src,
                dst,
                TestEdge::Labeled,
                Some(PropertyValue::String(format!("edge-{i}-{k}"))),
            );
        }
    }
    diff
}

/// Builds a diff like [`build_bulk_diff`], but spreading edges over five edge kinds
/// so more than one edge storage slot is populated.
pub fn build_multi_kind_diff(node_count: usize, edges_per_node: usize) -> GraphDiff<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let ids: Vec<usize> = (0..node_count)
        .map(|i| diff.add_node(node_for(i)))
        .collect();

    let stride = (node_count / (edges_per_node + 1)).max(1);
    for (i, &src) in ids.iter().enumerate() {
        for k in 1..=edges_per_node {
            let dst = ids[(i + k * stride) % node_count];
            let (kind, value) = match (i + k) % 5 {
                0 => (TestEdge::Plain, None),
                1 => (
                    TestEdge::Labeled,
                    Some(PropertyValue::String(format!("edge-{i}-{k}"))),
                ),
                2 => (TestEdge::Active, Some(PropertyValue::Bool(i % 2 == 0))),
                3 => (TestEdge::Weight, Some(PropertyValue::Byte((i % 256) as u8))),
                _ => (TestEdge::Distance, Some(PropertyValue::Int(i as i32))),
            };
            diff.add_edge(src, dst, kind, value);
        }
    }
    diff
}
