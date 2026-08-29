use flatpg::{
    error::Error,
    graph::{
        Graph,
        builder::{GraphDiff, QuantifiedProperty},
    },
    node::{NodeId, RawNodeId},
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::setup_three_file_nodes;

/// Locks in a documented gotcha from `GraphDiff::apply`'s doc comment: `update_node_property`
/// never checks whether the node was already deleted by an earlier diff, so applying a
/// property update against a node tombstoned in a prior `apply` call silently succeeds
/// rather than erroring.
#[test]
fn update_node_property_on_node_deleted_by_earlier_diff_succeeds() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "a.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut remove = GraphDiff::<TestSchema>::default();
    remove.remove_node(&node);
    let (graph, _) = remove.apply(graph).expect("apply remove");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 0);

    let mut update = GraphDiff::<TestSchema>::default();
    update.update_node_property(
        &node,
        TestProperty::Key,
        QuantifiedProperty::One(PropertyValue::String("still-writable.rs".to_string())),
    );
    let (graph, _) = update.apply(graph).expect("apply update on deleted node");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        AlphaNode::new(&graph, node.seq()).key().unwrap(),
        "still-writable.rs"
    );
}

/// `update_node_property` targets an `(existing node, property slot)` cell in the
/// current graph, unlike `remove_node`/`add_edge` which resolve their references
/// against a shared node-existence notion. A seq that was never created for its kind
/// is out of range for that cell, so it errors rather than silently no-opping.
#[test]
fn update_node_property_on_node_seq_never_created_is_an_error() {
    let graph = setup_three_file_nodes();
    let phantom = RawNodeId::new(TestNode::Alpha.index(), 9999);

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.update_node_property(
        phantom,
        TestProperty::Key,
        QuantifiedProperty::One(PropertyValue::String("nowhere.rs".to_string())),
    );
    let err = diff.apply(graph).err().expect("expected an error");
    assert!(matches!(err, Error::NodeOffsetNotFound(_)));
}

/// Companion to the above: `remove_node` takes the opposite, silently-forgiving path for
/// the same kind of bad reference. Pinning both down together documents that the two
/// "existing node" mutations in this API do not treat an out-of-range seq the same way.
#[test]
fn remove_node_with_seq_never_created_is_a_silent_no_op() {
    let graph = setup_three_file_nodes();
    let phantom = RawNodeId::new(TestNode::Alpha.index(), 9999);

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(phantom);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 3);
}

#[test]
fn remove_first_of_many_nodes_preserves_others() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(&nodes[0]);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 2);
    let remaining: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();
    assert_eq!(
        AlphaNode::new(&graph, remaining[0].seq()).key().unwrap(),
        "b.rs"
    );
    assert_eq!(
        AlphaNode::new(&graph, remaining[1].seq()).key().unwrap(),
        "c.rs"
    );
}

#[test]
fn remove_middle_of_many_nodes_preserves_others() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(&nodes[1]);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 2);
    let remaining: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();
    assert_eq!(
        AlphaNode::new(&graph, remaining[0].seq()).key().unwrap(),
        "a.rs"
    );
    assert_eq!(
        AlphaNode::new(&graph, remaining[1].seq()).key().unwrap(),
        "c.rs"
    );
}

#[test]
fn remove_last_of_many_nodes_preserves_others() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(&nodes[2]);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 2);
    let remaining: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();
    assert_eq!(
        AlphaNode::new(&graph, remaining[0].seq()).key().unwrap(),
        "a.rs"
    );
    assert_eq!(
        AlphaNode::new(&graph, remaining[1].seq()).key().unwrap(),
        "b.rs"
    );
}

#[test]
fn add_node_remove_then_add_new_node_is_accessible() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "first.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.remove_node(&node);
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 0);

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "second.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff3.apply(graph).expect("apply diff 3");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    let remaining = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    assert_eq!(
        AlphaNode::new(&graph, remaining.seq()).key().unwrap(),
        "second.rs"
    );
}

#[test]
fn add_property_remove_then_readd_restores_value() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "original.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.update_node_property(&node, TestProperty::Key, QuantifiedProperty::Multi(vec![]));
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(
        graph
            .get_node_property(RawNodeId::from(&node), TestProperty::Key)
            .unwrap()
            .count(),
        0
    );

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.update_node_property(
        &node,
        TestProperty::Key,
        QuantifiedProperty::One(PropertyValue::String("restored.rs".to_string())),
    );
    let (graph, _) = diff3.apply(graph).expect("apply diff 3");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        AlphaNode::new(&graph, node.seq()).key().unwrap(),
        "restored.rs"
    );
}
