use flatpg::{
    edge::Direction,
    error::Error,
    graph::{Graph, builder::GraphDiff},
    node::RawNodeId,
    prelude::*,
};
use test_fixtures::*;

use crate::common::{collect_edges, out_edge_dst_seqs, setup_graph_with_fan_out_edges};

/// Locks in the other documented gotcha from `GraphDiff::apply`'s doc comment: a stale
/// `EdgeId` from before an earlier `apply` call's own removal can make `remove_edge`'s
/// position-based and neighbor-based sides disagree about which edge to remove, leaving
/// a dangling half-edge that fails `check_integrity` rather than an error at `apply` time.
#[test]
fn remove_edge_with_id_captured_before_earlier_removal_corrupts_the_graph() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta1, beta2) = (betas[1], betas[2]);

    let mut edges = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out);
    edges.sort_by_key(|e| e.seq());
    // Captured against the pre-removal graph: `stale` names whatever edge is at local
    // position 1 (beta1) right now. `first` (position 0, beta0) is removed below.
    let stale = edges.remove(1);
    let first = edges.remove(0);
    let stale_local_seq = stale.seq();
    assert_eq!(stale.dst_node().seq(), beta1.seq());

    let mut remove_first = GraphDiff::<TestSchema>::default();
    remove_first.remove_edge(first);
    let (graph, _) = remove_first.apply(graph).expect("apply first removal");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    // beta2's edge has now shifted down into position 1, the position `stale` points at.
    let shifted = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.seq() == stale_local_seq)
        .expect("an edge now occupies the stale position");
    assert_eq!(shifted.dst_node().seq(), beta2.seq());

    let mut remove_stale = GraphDiff::<TestSchema>::default();
    remove_stale.remove_edge(stale);
    let (graph, _) = remove_stale
        .apply(graph)
        .expect("apply diff built with the stale id");

    // Position-based primary side: local position 1 is now beta2's edge, so alpha's Out
    // list loses it. Neighbor-based secondary side: it re-searches for `stale`'s own
    // recorded neighbor, beta1, so beta1's In list loses its (still-live) entry instead.
    // beta2's In entry is left dangling with no matching Out entry on alpha.
    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts, vec![beta1.seq()]);
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta1), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta2), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
    assert!(matches!(
        graph.check_integrity(),
        Err(Error::ReverseEdgeNotFound { .. })
    ));
}

#[test]
fn remove_first_of_many_out_edges_preserves_others() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b0 = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.dst_node().seq() == beta0.seq())
        .expect("edge to beta0");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b0);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&beta1.seq()));
    assert!(dsts.contains(&beta2.seq()));
    assert!(!dsts.contains(&beta0.seq()));

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta0), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta1), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta2), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
}

#[test]
fn remove_middle_of_many_out_edges_preserves_others() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b1 = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.dst_node().seq() == beta1.seq())
        .expect("edge to beta1");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b1);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&beta0.seq()));
    assert!(dsts.contains(&beta2.seq()));
    assert!(!dsts.contains(&beta1.seq()));

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta1), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}

#[test]
fn remove_last_of_many_out_edges_preserves_others() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b2 = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out)
        .into_iter()
        .find(|e| e.dst_node().seq() == beta2.seq())
        .expect("edge to beta2");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b2);
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&beta0.seq()));
    assert!(dsts.contains(&beta1.seq()));
    assert!(!dsts.contains(&beta2.seq()));

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta2), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}

#[test]
fn add_edge_remove_then_readd_edge_is_accessible() {
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

    let edges = collect_edges(&graph, alpha, TestEdge::Plain, Direction::Out);
    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.remove_edge(edges.into_iter().next().unwrap());
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&alpha), TestEdge::Plain, Direction::Out)
            .unwrap(),
        0
    );

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.add_edge(
        RawNodeId::from(&alpha),
        RawNodeId::from(&beta),
        TestEdge::Plain,
        None,
    );
    let (graph, _) = diff3.apply(graph).expect("apply diff 3");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&alpha), TestEdge::Plain, Direction::Out)
            .unwrap(),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(RawNodeId::from(&beta), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
}
