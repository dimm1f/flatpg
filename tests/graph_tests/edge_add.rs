use flatpg::{
    edge::Direction,
    error::Error,
    graph::{Graph, builder::GraphDiff},
    node::{NodeId, RawNodeId},
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::{collect_edges, out_edge_dst_seqs, setup_three_file_nodes, string_value};

#[test]
fn add_edge_between_new_nodes() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_id, beta_id, TestEdge::Plain, None);

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&alpha), TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta), TestEdge::Plain, Direction::In)
            .expect("in edges count"),
        1
    );
}

#[test]
fn add_edge_endpoints_are_correct() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_id, beta_id, TestEdge::Plain, None);

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");

    let out_edges = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out);
    assert_eq!(out_edges.len(), 1);
    assert_eq!(out_edges[0].src_node().kind(), TestNode::Alpha);
    assert_eq!(out_edges[0].src_node().seq(), alpha.seq());
    assert_eq!(out_edges[0].dst_node().kind(), TestNode::Beta);
    assert_eq!(out_edges[0].dst_node().seq(), beta.seq());
}

#[test]
fn add_multiple_edges_same_kind() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta1_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    let beta2_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_id, beta1_id, TestEdge::Plain, None);
    diff.add_edge(alpha_id, beta2_id, TestEdge::Plain, None);

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&alpha), TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        2
    );
}

#[test]
fn add_edge_between_existing_nodes() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    setup.add_node(builders::BetaNodeBuilder::new().build());
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha_ref = RawNodeId::from(
        &graph
            .nodes_by_kind(TestNode::Alpha)
            .next()
            .expect("Alpha node"),
    );
    let beta_ref = RawNodeId::from(
        &graph
            .nodes_by_kind(TestNode::Beta)
            .next()
            .expect("Beta node"),
    );

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_edge(alpha_ref, beta_ref, TestEdge::Plain, None);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        graph
            .get_edges_count(alpha_ref, TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(beta_ref, TestEdge::Plain, Direction::In)
            .expect("in edges count"),
        1
    );
}

#[test]
fn add_edge_between_new_and_existing_node() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha_ref = RawNodeId::from(
        &graph
            .nodes_by_kind(TestNode::Alpha)
            .next()
            .expect("Alpha node"),
    );

    let mut diff = GraphDiff::<TestSchema>::default();
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_ref, beta_id, TestEdge::Plain, None);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    assert_eq!(
        graph
            .get_edges_count(alpha_ref, TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta), TestEdge::Plain, Direction::In)
            .expect("in edges count"),
        1
    );
}

#[test]
fn add_edge_from_new_node_to_existing_node_in_middle_of_seq_range() {
    let graph = setup_three_file_nodes();
    let alpha_nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();
    let middle_ref = RawNodeId::from(&alpha_nodes[1]);

    let mut diff = GraphDiff::<TestSchema>::default();
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(beta_id, middle_ref, TestEdge::Plain, None);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta), TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(middle_ref, TestEdge::Plain, Direction::In)
            .expect("in edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(
                RawNodeId::from(&alpha_nodes[0]),
                TestEdge::Plain,
                Direction::In
            )
            .expect("in edges count"),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(
                RawNodeId::from(&alpha_nodes[2]),
                TestEdge::Plain,
                Direction::In
            )
            .expect("in edges count"),
        0
    );
}

/// Locks in a fix for silent corruption: a missing property on a property-carrying
/// edge kind used to be skipped while the neighbor still counted toward the slot's
/// offsets, leaving `values` shorter than `neighbors` and shifting every later edge's
/// property onto the wrong edge.
#[test]
fn add_edge_without_property_for_property_carrying_kind_is_rejected() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_id, beta_id, TestEdge::Labeled, None);

    let err = diff.apply(Graph::new()).err().expect("expected an error");
    assert!(matches!(err, Error::InvalidPropertyType { .. }));
}

/// Companion to the above: the corruption only became observable once a later edge in
/// the same slot carried a property, so the mixed case is the one that silently
/// returned another edge's value.
#[test]
fn add_edge_mixing_present_and_missing_properties_is_rejected() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha0 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let alpha1 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha0, beta_id, TestEdge::Labeled, None);
    diff.add_edge(
        alpha1,
        beta_id,
        TestEdge::Labeled,
        Some(PropertyValue::String("p1".into())),
    );

    let err = diff.apply(Graph::new()).err().expect("expected an error");
    assert!(matches!(err, Error::InvalidPropertyType { .. }));
}

/// The mirror of the two cases above: an edge kind that stores no property would
/// silently discard one, so supplying it is rejected rather than ignored.
#[test]
fn add_edge_with_property_for_propertyless_kind_is_rejected() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Plain,
        Some(PropertyValue::String("dropped".into())),
    );

    let err = diff.apply(Graph::new()).err().expect("expected an error");
    assert!(matches!(err, Error::InvalidPropertyType { .. }));
}

#[test]
fn add_edge_with_property_stores_property() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Labeled,
        Some(PropertyValue::String("x".to_string())),
    );

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let edges = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out);
    assert_eq!(edges.len(), 1);

    let property = graph
        .get_edge_property(edges.into_iter().next().unwrap())
        .expect("edge property lookup")
        .expect("edge property should be set");
    assert_eq!(string_value(&graph, property), "x");
}

/// Regression test for a panic: `add_edge` accepted a `RawNodeId` built with a valid
/// node kind but a seq that was never created (neither an existing node nor one of this
/// diff's own new nodes), and `prepare` indexed a per-slot histogram sized to the real
/// node count with that seq directly, panicking instead of erroring or dropping the
/// edge the way every other bad-reference case in this file does.
#[test]
fn add_edge_to_node_seq_never_created_is_dropped() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let phantom = RawNodeId::new(TestNode::Alpha.index(), 9999);
    diff.add_edge(alpha_id, phantom, TestEdge::Plain, None);

    let (graph, ids) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(
        graph
            .get_edges_count(
                RawNodeId::from(&ids[alpha_id]),
                TestEdge::Plain,
                Direction::Out
            )
            .expect("out edges count"),
        0
    );
}

/// Companion to the above: a `RawNodeId` with a kind index that doesn't name any
/// registered node kind at all is the other flavor of bad reference. It already took
/// the graceful path (silently dropped, via `TryFrom<RawNodeId> for NodeId<S>` failing)
/// before the fix above; this pins that down as a passing case rather than one that
/// happens to avoid the same panic for different reasons.
#[test]
fn add_edge_to_unresolvable_node_kind_is_dropped() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let bogus_kind = RawNodeId::new(999, 0);
    diff.add_edge(alpha_id, bogus_kind, TestEdge::Plain, None);

    let (graph, ids) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(
        graph
            .get_edges_count(
                RawNodeId::from(&ids[alpha_id]),
                TestEdge::Plain,
                Direction::Out
            )
            .expect("out edges count"),
        0
    );
}

#[test]
fn add_edge_to_node_removed_in_same_diff_is_dropped() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha_nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();
    let removed_ref = RawNodeId::from(&alpha_nodes[0]);
    let kept_ref = RawNodeId::from(&alpha_nodes[1]);

    let mut diff = GraphDiff::<TestSchema>::default();
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.remove_node(removed_ref);
    diff.add_edge(beta_id, removed_ref, TestEdge::Plain, None);
    diff.add_edge(beta_id, kept_ref, TestEdge::Plain, None);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);

    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta), TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(kept_ref, TestEdge::Plain, Direction::In)
            .expect("in edges count"),
        1
    );
}

/// Pins the within-node edge order to `add_edge` call order. `EdgeId::seq()` is the
/// local position in a node's adjacency run, so any reordering inside the builder
/// silently renumbers edge ids — nothing else in the suite would catch that.
#[test]
fn out_edges_of_one_node_keep_add_edge_order() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_ids: Vec<_> = (0..4)
        .map(|_| diff.add_node(builders::BetaNodeBuilder::new().build()))
        .collect();
    // Deliberately not ascending, so a sort by destination would not reproduce it.
    for &i in &[2usize, 0, 3, 1] {
        diff.add_edge(alpha_id, beta_ids[i], TestEdge::Plain, None);
    }
    let (graph, ids) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = ids[alpha_id];
    let expected: Vec<usize> = [2usize, 0, 3, 1]
        .iter()
        .map(|&i| ids[beta_ids[i]].seq())
        .collect();
    assert_eq!(out_edge_dst_seqs(&graph, alpha), expected);
}

/// Companion to the above: half-edges for different owning nodes interleave in
/// `add_edge` order, so regrouping them by owner must preserve each owner's own
/// relative order rather than the global call order.
#[test]
fn interleaved_out_edges_keep_per_node_add_edge_order() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha0 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let alpha1 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_ids: Vec<_> = (0..2)
        .map(|_| diff.add_node(builders::BetaNodeBuilder::new().build()))
        .collect();

    diff.add_edge(alpha0, beta_ids[1], TestEdge::Plain, None);
    diff.add_edge(alpha1, beta_ids[0], TestEdge::Plain, None);
    diff.add_edge(alpha0, beta_ids[0], TestEdge::Plain, None);
    diff.add_edge(alpha1, beta_ids[1], TestEdge::Plain, None);

    let (graph, ids) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        out_edge_dst_seqs(&graph, ids[alpha0]),
        vec![ids[beta_ids[1]].seq(), ids[beta_ids[0]].seq()]
    );
    assert_eq!(
        out_edge_dst_seqs(&graph, ids[alpha1]),
        vec![ids[beta_ids[0]].seq(), ids[beta_ids[1]].seq()]
    );
}
