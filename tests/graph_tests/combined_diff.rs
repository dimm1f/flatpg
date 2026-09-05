use flatpg::{
    edge::Direction,
    graph::{
        Graph,
        builder::{GraphDiff, QuantifiedProperty},
    },
    node::{NodeId, RawNodeId},
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::{collect_edges, out_edge_dst_seqs, setup_graph_with_fan_out_edges};

#[test]
fn remove_existing_and_add_new_edge_sharing_an_edge_slot_in_one_diff() {
    // `alpha`'s out-edge slot is touched twice in the same diff: once by removing an
    // existing edge, once by adding a new edge to a brand-new node. `apply`'s new-edge
    // insertion must start from the post-removal offsets, not the graph's pre-diff
    // offsets, or the new edge lands at the wrong position (or panics).
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b1 = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.dst_node().seq() == beta1.seq())
        .expect("edge to beta1");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b1);
    let new_beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha, new_beta, TestEdge::Plain, None);

    let (graph, ids) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let new_beta_id = ids[0];

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 3);
    assert!(dsts.contains(&beta0.seq()));
    assert!(!dsts.contains(&beta1.seq()));
    assert!(dsts.contains(&beta2.seq()));
    assert!(dsts.contains(&new_beta_id.seq()));

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta1), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(
                RawNodeId::from(&new_beta_id),
                TestEdge::Plain,
                Direction::In
            )
            .unwrap(),
        1
    );
}

#[test]
fn update_existing_and_add_new_node_sharing_a_property_slot_in_one_diff() {
    // The Alpha/Values property slot is touched twice in the same diff: once by
    // updating an existing node's Multi property (changing its element count), once
    // by adding a brand-new Alpha node with its own Values. The new node's property
    // batch must be appended after the *updated* offsets, not the graph's pre-diff
    // offsets, or its values land at the wrong position (or panic).
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "a0".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "a1".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let existing: NodeId<TestSchema> = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.update_node_property(
        &existing,
        TestProperty::Values,
        QuantifiedProperty::Multi(
            ["z0", "z1", "z2"]
                .into_iter()
                .map(|v| PropertyValue::String(v.to_string()))
                .collect(),
        ),
    );
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "b0".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "b1".to_string())
            .unwrap()
            .build(),
    );
    let (graph, ids) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let new_node = ids[0];

    assert_eq!(
        AlphaNode::new(&graph, existing.seq()).values().unwrap(),
        vec!["z0", "z1", "z2"]
    );
    assert_eq!(
        AlphaNode::new(&graph, new_node.seq()).values().unwrap(),
        vec!["b0", "b1"]
    );
}

#[test]
fn update_same_property_multiple_times_in_one_diff_keeps_only_last_value() {
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

    // Three updates to the same (node, property) in one diff: shrink, grow past the
    // original length, then shrink again. Only the last one should survive, using the
    // original pre-diff count as the offset-shift baseline, not any intermediate one.
    let mut updates = GraphDiff::<TestSchema>::default();
    updates.update_node_property(
        &nodes[0],
        TestProperty::Values,
        QuantifiedProperty::Multi(vec![PropertyValue::String("z0".to_string())]),
    );
    updates.update_node_property(
        &nodes[0],
        TestProperty::Values,
        QuantifiedProperty::Multi(
            ["w0", "w1", "w2", "w3"]
                .into_iter()
                .map(|v| PropertyValue::String(v.to_string()))
                .collect(),
        ),
    );
    updates.update_node_property(
        &nodes[0],
        TestProperty::Values,
        QuantifiedProperty::Multi(vec![PropertyValue::String("final".to_string())]),
    );
    let (graph, _) = updates.apply(graph).expect("apply updates");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        AlphaNode::new(&graph, nodes[0].seq()).values().unwrap(),
        vec!["final"]
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

#[test]
fn remove_two_out_edges_from_same_node_in_ascending_order_in_one_diff() {
    let (graph, alpha, _betas) = setup_graph_with_fan_out_edges();

    let mut edges = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out);
    edges.sort_by_key(|e| e.seq());
    let removed_dsts: Vec<NodeId<TestSchema>> = edges[..2].iter().map(|e| e.dst_node()).collect();
    let survivor = edges[2].dst_node();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edges.remove(0));
    diff.remove_edge(edges.remove(0));
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(out_edge_dst_seqs(&graph, alpha), vec![survivor.seq()]);
    for removed in removed_dsts {
        assert_eq!(
            graph
                .get_edges_count(RawNodeId::from(&removed), TestEdge::Plain, Direction::In)
                .unwrap(),
            0
        );
    }
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&survivor), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
}

#[test]
fn apply_mixed_scattered_changes_to_one_diff_updates_each_node_correctly() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_ids: Vec<_> = (0..4)
        .map(|i| {
            setup.add_node(
                builders::BetaNodeBuilder::new()
                    .add_property(TestProperty::Count, i)
                    .unwrap()
                    .build(),
            )
        })
        .collect();
    for &beta_id in &beta_ids {
        setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    }
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let betas: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Beta).collect();

    let edge_to_beta0 = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.dst_node().seq() == betas[0].seq())
        .expect("edge to beta0");
    let edge_to_beta2 = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.dst_node().seq() == betas[2].seq())
        .expect("edge to beta2");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_beta0);
    diff.remove_edge(edge_to_beta2);
    diff.update_node_property(
        &betas[1],
        TestProperty::Count,
        QuantifiedProperty::One(PropertyValue::Int(100)),
    );
    diff.update_node_property(
        &betas[3],
        TestProperty::Count,
        QuantifiedProperty::One(PropertyValue::Int(300)),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&betas[1].seq()));
    assert!(dsts.contains(&betas[3].seq()));

    assert_eq!(BetaNode::new(&graph, betas[0].seq()).count().unwrap(), 0);
    assert_eq!(BetaNode::new(&graph, betas[1].seq()).count().unwrap(), 100);
    assert_eq!(BetaNode::new(&graph, betas[2].seq()).count().unwrap(), 2);
    assert_eq!(BetaNode::new(&graph, betas[3].seq()).count().unwrap(), 300);

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&betas[0]), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&betas[2]), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}

#[test]
fn node_as_primary_in_one_removed_edge_and_secondary_in_another_in_one_diff() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let a_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let b_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    let c_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    setup.add_edge(a_id, b_id, TestEdge::Plain, None);
    setup.add_edge(b_id, c_id, TestEdge::Plain, None);
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alphas: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();
    let (a, c) = (alphas[0], alphas[1]);
    let b = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");

    let edge_a_to_b = collect_edges(&graph, a, TestEdge::Plain, Direction::Out)
        .into_iter()
        .next()
        .expect("edge a->b");
    let edge_b_to_c = collect_edges(&graph, b, TestEdge::Plain, Direction::Out)
        .into_iter()
        .next()
        .expect("edge b->c");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_a_to_b); // b is secondary here: its In slot is touched
    diff.remove_edge(edge_b_to_c); // b is primary here: its Out slot is touched
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&a), TestEdge::Plain, Direction::Out)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&b), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&b), TestEdge::Plain, Direction::Out)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&c), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}

#[test]
fn removing_the_same_edge_twice_in_one_diff_collapses_into_one_removal() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
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
    // Two independent EdgeId lookups against the same untouched graph, both resolving to
    // the same underlying half-edge.
    let first = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .next()
        .expect("edge a->b");
    let second = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .next()
        .expect("edge a->b");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(first);
    diff.remove_edge(second);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&alpha), TestEdge::Plain, Direction::Out)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}
