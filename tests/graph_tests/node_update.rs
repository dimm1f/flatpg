use flatpg::{
    graph::{
        Graph,
        builder::{GraphDiff, QuantifiedProperty},
    },
    node::NodeId,
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::setup_three_file_nodes;

#[test]
fn update_first_of_many_nodes_leaves_others_unchanged() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.update_node_property(
        &nodes[0],
        TestProperty::Key,
        QuantifiedProperty::One(PropertyValue::String("updated.rs".to_string())),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "updated.rs");
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "b.rs");
    assert_eq!(AlphaNode::new(&graph, 2).key().unwrap(), "c.rs");
}

#[test]
fn update_middle_of_many_nodes_leaves_others_unchanged() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.update_node_property(
        &nodes[1],
        TestProperty::Key,
        QuantifiedProperty::One(PropertyValue::String("updated.rs".to_string())),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "a.rs");
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "updated.rs");
    assert_eq!(AlphaNode::new(&graph, 2).key().unwrap(), "c.rs");
}

#[test]
fn update_last_of_many_nodes_leaves_others_unchanged() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.update_node_property(
        &nodes[2],
        TestProperty::Key,
        QuantifiedProperty::One(PropertyValue::String("updated.rs".to_string())),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "a.rs");
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "b.rs");
    assert_eq!(AlphaNode::new(&graph, 2).key().unwrap(), "updated.rs");
}

#[test]
fn update_multi_valued_property_shrink_then_grow_leaves_siblings_unchanged() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "a0".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "a1".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "a2".to_string())
            .unwrap()
            .build(),
    );
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "b0".to_string())
            .unwrap()
            .build(),
    );
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "c0".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "c1".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    // Shrink node 0's Values from 3 entries down to 1. The offsets loop in
    // `apply_changes`'s `UpdateNodeProperty` branch must shift nodes 1 and 2's
    // offsets left by 2, or their `Values` would read from the wrong slice.
    let mut shrink = GraphDiff::<TestSchema>::default();
    shrink.update_node_property(
        &nodes[0],
        TestProperty::Values,
        QuantifiedProperty::Multi(vec![PropertyValue::String("z0".to_string())]),
    );
    let (graph, _) = shrink.apply(graph).expect("apply shrink");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        AlphaNode::new(&graph, nodes[0].seq()).values().unwrap(),
        vec!["z0"]
    );
    assert_eq!(
        AlphaNode::new(&graph, nodes[1].seq()).values().unwrap(),
        vec!["b0"]
    );
    assert_eq!(
        AlphaNode::new(&graph, nodes[2].seq()).values().unwrap(),
        vec!["c0", "c1"]
    );

    // Grow node 0's Values from 1 entry back up past its original length (4).
    // Same offsets loop, opposite branch: nodes 1 and 2 must shift right by 3.
    let mut grow = GraphDiff::<TestSchema>::default();
    grow.update_node_property(
        &nodes[0],
        TestProperty::Values,
        QuantifiedProperty::Multi(
            ["w0", "w1", "w2", "w3"]
                .into_iter()
                .map(|v| PropertyValue::String(v.to_string()))
                .collect(),
        ),
    );
    let (graph, _) = grow.apply(graph).expect("apply grow");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        AlphaNode::new(&graph, nodes[0].seq()).values().unwrap(),
        vec!["w0", "w1", "w2", "w3"]
    );
    assert_eq!(
        AlphaNode::new(&graph, nodes[1].seq()).values().unwrap(),
        vec!["b0"]
    );
    assert_eq!(
        AlphaNode::new(&graph, nodes[2].seq()).values().unwrap(),
        vec!["c0", "c1"]
    );
}
