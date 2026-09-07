use flatpg::{
    edge::Direction,
    graph::{Graph, builder::GraphDiff},
    node::RawNodeId,
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::{
    collect_edges, out_edge_dst_seqs, setup_graph_with_fan_out_edges, string_value,
};

/// An `EdgeId` captured before an earlier removal still names its own edge.
///
/// A removal shifts every later edge on the same node down a position, so the id's recorded
/// position stops naming the edge it was taken from. Removal treats that position as a hint
/// only: for a kind carrying no property it re-scans by the endpoint the id records, so the
/// stale id removes the edge it always meant rather than whichever one moved into its slot.
#[test]
fn remove_edge_with_id_captured_before_earlier_removal_still_removes_its_own_edge() {
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

    // beta1's edge is gone — the one the stale id named — and beta2's, which had moved into
    // its position, is untouched on both sides.
    assert_eq!(out_edge_dst_seqs(&graph, alpha), vec![beta2.seq()]);
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
    graph
        .check_integrity()
        .expect("graph passes integrity check");
}

/// The same for a kind that does carry a property: there the id's own `EdgeSeq` names the
/// edge outright, so no endpoint scan is needed to get it right.
#[test]
fn stale_edge_id_of_a_valued_kind_removes_the_edge_its_identity_names() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = setup.add_node(builders::BetaNodeBuilder::new().build());
    for value in ["p0", "p1", "p2"] {
        setup.add_edge(
            alpha,
            beta,
            TestEdge::Labeled,
            Some(PropertyValue::String(value.to_string())),
        );
    }
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let mut edges = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out);
    edges.sort_by_key(|e| e.seq());
    let stale = edges.remove(2);
    let first = edges.remove(0);
    assert!(stale.edge_seq().is_some(), "a valued kind carries identity");

    let mut remove_first = GraphDiff::<TestSchema>::default();
    remove_first.remove_edge(first);
    let (graph, _) = remove_first.apply(graph).expect("apply first removal");

    let mut remove_stale = GraphDiff::<TestSchema>::default();
    remove_stale.remove_edge(stale);
    let (graph, _) = remove_stale
        .apply(graph)
        .expect("apply diff built with the stale id");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    // "p0" went with the first removal and "p2" is the one the stale id named, so only the
    // edge it never referred to survives.
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let surviving: Vec<String> = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out)
        .into_iter()
        .map(|edge| {
            string_value(
                &graph,
                graph
                    .get_edge_property(edge)
                    .expect("edge property lookup")
                    .next()
                    .expect("a Labeled edge carries a value"),
            )
        })
        .collect();
    assert_eq!(surviving, vec!["p1"]);
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

/// Removing one of two parallel edges must leave the survivor's two halves agreeing on their
/// property value.
#[test]
fn removing_one_of_two_parallel_edges_keeps_both_halves_of_the_survivor_in_agreement() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    for value in ["p0", "p1"] {
        setup.add_edge(
            alpha_id,
            beta_id,
            TestEdge::Labeled,
            Some(PropertyValue::String(value.to_string())),
        );
    }
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph.nodes_by_kind(TestNode::Alpha).next().expect("Alpha");
    let beta = graph.nodes_by_kind(TestNode::Beta).next().expect("Beta");

    let mut out_edges = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out);
    out_edges.sort_by_key(|e| e.seq());
    let removed = out_edges.remove(1);
    assert_eq!(
        string_value(
            &graph,
            graph.get_edge_property(removed).unwrap().next().unwrap()
        ),
        "p1"
    );

    let mut out_edges = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out);
    out_edges.sort_by_key(|e| e.seq());
    let mut remove = GraphDiff::<TestSchema>::default();
    remove.remove_edge(out_edges.remove(1));
    let (graph, _) = remove.apply(graph).expect("apply removal");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut survivor_out = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out);
    let mut survivor_in = collect_edges(&graph, beta, TestEdge::Labeled, Direction::In);
    assert_eq!(survivor_out.len(), 1);
    assert_eq!(survivor_in.len(), 1);

    let from_out = string_value(
        &graph,
        graph
            .get_edge_property(survivor_out.remove(0))
            .unwrap()
            .next()
            .unwrap(),
    );
    let from_in = string_value(
        &graph,
        graph
            .get_edge_property(survivor_in.remove(0))
            .unwrap()
            .next()
            .unwrap(),
    );
    assert_eq!(
        from_out, "p0",
        "the surviving edge is the one that was not removed"
    );
    assert_eq!(
        from_out, from_in,
        "both halves of one edge must report the same property"
    );
}
