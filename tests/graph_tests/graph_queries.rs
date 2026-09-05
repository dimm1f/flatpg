use flatpg::{
    edge::Direction,
    graph::{Graph, builder::GraphDiff},
    node::{NodeId, RawNodeId},
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::{collect_edges, setup_graph_with_fan_out_edges, setup_three_file_nodes};

#[test]
fn graph_default_matches_a_freshly_built_empty_graph() {
    let graph = Graph::<TestSchema>::default();
    graph
        .check_integrity()
        .expect("default graph passes integrity check");
    assert_eq!(graph.node_count(), 0);
}

#[test]
fn node_count_sums_node_count_by_kind_across_all_kinds() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(builders::AlphaNodeBuilder::new().build());
    diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_node(builders::BetaNodeBuilder::new().build());
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(graph.node_count_by_kind(TestNode::Beta), 2);
    assert_eq!(graph.node_count(), 3);
}

#[test]
fn nodes_by_kind_with_deleted_still_lists_a_tombstoned_node_as_deleted() {
    let graph = setup_three_file_nodes();
    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(&node);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind_with_deleted(TestNode::Alpha), 3);
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 2);

    let all: Vec<NodeId<TestSchema>> = graph.nodes_by_kind_with_deleted(TestNode::Alpha).collect();
    assert_eq!(all.len(), 3);
    assert!(graph.is_node_deleted(all[0]));
    assert!(!graph.is_node_deleted(all[1]));
}

#[test]
fn is_node_deleted_is_true_for_a_seq_that_was_never_created() {
    let graph = setup_three_file_nodes();
    let phantom: NodeId<TestSchema> = RawNodeId::new(TestNode::Alpha.index(), 9999)
        .try_into()
        .unwrap();
    assert!(graph.is_node_deleted(phantom));
}

#[test]
fn node_id_display_shows_the_kind_label_and_seq() {
    let graph = setup_three_file_nodes();
    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    assert_eq!(node.to_string(), format!("NodeId(Alpha,{})", node.seq()));
}

#[test]
fn get_edges_count_matches_the_number_of_out_edges_and_zero_for_a_phantom_seq() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&alpha), TestEdge::Plain, Direction::Out)
            .unwrap(),
        betas.len()
    );

    let phantom = RawNodeId::new(TestNode::Alpha.index(), 9999);
    assert_eq!(
        graph
            .get_edges_count(phantom, TestEdge::Plain, Direction::Out)
            .unwrap(),
        0
    );
}

#[test]
fn resolve_property_converts_every_stored_property_variant() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::GammaNodeBuilder::new()
            .add_property(TestProperty::Flag, true)
            .unwrap()
            .add_property(TestProperty::Level, 7u8)
            .unwrap()
            .add_property(TestProperty::Rank, 42i16)
            .unwrap()
            .add_property(TestProperty::BigCount, 1_000_000_000_000i64)
            .unwrap()
            .add_property(TestProperty::Ratio, 1.5f32)
            .unwrap()
            .add_property(TestProperty::Score, 2.5f64)
            .unwrap()
            .add_property(TestProperty::Tags, Status::Active)
            .unwrap()
            .add_property(TestProperty::LinkedNode, alpha)
            .unwrap()
            .build(),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let gamma = graph
        .nodes_by_kind(TestNode::Gamma)
        .next()
        .expect("Gamma node");
    let raw = RawNodeId::from(&gamma);
    let resolved = |prop_kind| {
        let stored = graph
            .get_node_property(raw, prop_kind)
            .unwrap()
            .next()
            .unwrap();
        graph.resolve_property(stored).unwrap()
    };

    assert!(matches!(
        resolved(TestProperty::Flag),
        PropertyValue::Bool(true)
    ));
    assert!(matches!(
        resolved(TestProperty::Level),
        PropertyValue::Byte(7)
    ));
    assert!(matches!(
        resolved(TestProperty::Rank),
        PropertyValue::Short(42)
    ));
    assert!(matches!(
        resolved(TestProperty::BigCount),
        PropertyValue::Long(1_000_000_000_000)
    ));
    assert!(matches!(resolved(TestProperty::Ratio), PropertyValue::Float(v) if v == 1.5));
    assert!(matches!(resolved(TestProperty::Score), PropertyValue::Double(v) if v == 2.5));
    assert!(matches!(
        resolved(TestProperty::Tags),
        PropertyValue::Enum(_)
    ));
    assert!(matches!(
        resolved(TestProperty::LinkedNode),
        PropertyValue::NodeId(_)
    ));
}

/// Locks the orientation contract `get_edge_property` depends on: whichever endpoint a
/// half-edge was queried from, `orient_edge` must hand that same node back.
#[test]
fn both_halves_of_an_edge_orient_back_to_the_node_they_were_queried_from() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha, beta, TestEdge::Plain, None);
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");

    for (queried, other, direction) in [(alpha, beta, Direction::Out), (beta, alpha, Direction::In)]
    {
        let edge = collect_edges(&graph, queried, TestEdge::Plain, direction)
            .into_iter()
            .next()
            .expect("one edge");

        assert_eq!(RawNodeId::from(&edge.src_node()), RawNodeId::from(&alpha));
        assert_eq!(RawNodeId::from(&edge.dst_node()), RawNodeId::from(&beta));

        let (near, near_direction, far, _) = edge
            .direction()
            .orient_edge((&edge.src_node()).into(), (&edge.dst_node()).into());
        assert_eq!(near, RawNodeId::from(&queried));
        assert_eq!(near_direction, direction);
        assert_eq!(far, RawNodeId::from(&other));
    }
}
