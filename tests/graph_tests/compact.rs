use flatpg::{
    edge::Direction,
    graph::{Graph, builder::GraphDiff, raw::RawGraph},
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::collect_edges;

fn labeled_values(graph: &Graph<TestSchema>) -> Vec<String> {
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    collect_edges(graph, alpha, TestEdge::Labeled, Direction::Out)
        .into_iter()
        .map(|edge_id| {
            LabeledEdge::new(
                graph,
                edge_id.src_node(),
                edge_id.dst_node(),
                edge_id.direction(),
                edge_id.seq(),
            )
            .property()
            .expect("edge property lookup")
            .to_string()
        })
        .collect()
}

fn three_labeled_edges() -> Graph<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    for value in ["p0", "p1", "p2"] {
        diff.add_edge(
            alpha,
            beta,
            TestEdge::Labeled,
            Some(PropertyValue::String(value.to_string())),
        );
    }
    let (graph, _) = diff.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    graph
}

/// Removing an edge drops its halves but leaves its values behind, because the write path only
/// ever appends. Compaction is what reclaims them.
#[test]
fn removing_an_edge_leaves_its_values_until_compaction() {
    let graph = three_labeled_edges();
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let middle = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out).remove(1);

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(middle);
    let (mut graph, _) = diff.apply(graph).expect("apply removal");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(labeled_values(&graph), vec!["p0", "p2"]);
    let raw: RawGraph<TestSchema> = graph.into();
    assert_eq!(
        raw.edge_property_storage[TestEdge::Labeled.index()].count(),
        3,
        "the removed edge's seq is still allocated"
    );

    graph = raw.try_into().expect("graph is still valid");
    graph
        .compact_edge_properties()
        .expect("compaction succeeds");
    graph
        .check_integrity()
        .expect("compacted graph passes integrity check");

    assert_eq!(labeled_values(&graph), vec!["p0", "p2"]);
    let raw: RawGraph<TestSchema> = graph.into();
    let store = &raw.edge_property_storage[TestEdge::Labeled.index()];
    assert_eq!(store.count(), 2, "the dead seq is reclaimed");
    assert_eq!(store.values().len(), 2);
}

/// A `Multi` kind's CSR offsets have to be rebuilt too, not just its values.
#[test]
fn compaction_rebuilds_a_multi_kinds_offsets() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    for values in [vec![1, 2], vec![3], vec![4, 5, 6]] {
        diff.add_edge_with(
            alpha,
            beta,
            TestEdge::Measurements,
            values
                .into_iter()
                .map(PropertyValue::Int)
                .collect::<Vec<_>>(),
        );
    }
    let (graph, _) = diff.apply(Graph::new()).expect("apply setup");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let first = collect_edges(&graph, alpha, TestEdge::Measurements, Direction::Out).remove(0);
    let mut removal = GraphDiff::<TestSchema>::default();
    removal.remove_edge(first);
    let (mut graph, _) = removal.apply(graph).expect("apply removal");

    graph
        .compact_edge_properties()
        .expect("compaction succeeds");
    graph
        .check_integrity()
        .expect("compacted graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let runs: Vec<Vec<i32>> = collect_edges(&graph, alpha, TestEdge::Measurements, Direction::Out)
        .into_iter()
        .map(|edge_id| {
            MeasurementsEdge::new(
                &graph,
                edge_id.src_node(),
                edge_id.dst_node(),
                edge_id.direction(),
                edge_id.seq(),
            )
            .property()
            .expect("edge property lookup")
        })
        .collect();
    assert_eq!(runs, vec![vec![3], vec![4, 5, 6]]);

    let raw: RawGraph<TestSchema> = graph.into();
    let store = &raw.edge_property_storage[TestEdge::Measurements.index()];
    assert_eq!(store.count(), 2);
    assert_eq!(store.values().len(), 4, "the dropped run's values are gone");
}

/// With nothing dead there is nothing to move, so compaction must leave the graph as it was
/// rather than renumbering into the same seqs.
#[test]
fn compaction_of_a_graph_with_no_removals_is_a_no_op() {
    let mut graph = three_labeled_edges();
    graph
        .compact_edge_properties()
        .expect("compaction succeeds");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(labeled_values(&graph), vec!["p0", "p1", "p2"]);
    let raw: RawGraph<TestSchema> = graph.into();
    assert_eq!(
        raw.edge_property_storage[TestEdge::Labeled.index()].count(),
        3
    );
}

/// Kinds carrying no property hand out no `EdgeSeq`, so compaction must simply skip them.
#[test]
fn compaction_leaves_none_typed_kinds_alone() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha, beta, TestEdge::Plain, None);
    diff.add_edge(alpha, beta, TestEdge::Plain, None);
    let (mut graph, _) = diff.apply(Graph::new()).expect("apply setup");

    graph
        .compact_edge_properties()
        .expect("compaction succeeds");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    assert_eq!(
        collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out).len(),
        2
    );
}
