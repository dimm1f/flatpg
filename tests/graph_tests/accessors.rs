use flatpg::{
    edge::{Direction, StoredEdge},
    graph::{Graph, builder::GraphDiff},
    node::StoredNode,
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::{collect_edges, string_value};

#[test]
fn edge_property_is_visible_from_both_endpoints() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha,
        beta,
        TestEdge::Labeled,
        Some(PropertyValue::String("p0".into())),
    );
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

    let mut out_edges = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out);
    assert_eq!(out_edges.len(), 1);
    let out_prop = graph
        .get_edge_property(out_edges.remove(0))
        .expect("edge property lookup")
        .expect("property from Out perspective");
    assert_eq!(string_value(&graph, out_prop), "p0");

    let mut in_edges = collect_edges(&graph, beta, TestEdge::Labeled, Direction::In);
    assert_eq!(in_edges.len(), 1);
    let in_prop = graph
        .get_edge_property(in_edges.remove(0))
        .expect("edge property lookup")
        .expect("property from In perspective");
    assert_eq!(string_value(&graph, in_prop), "p0");
}

#[test]
fn stored_edge_struct_and_edge_enum_match_graph_get_edges() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha,
        beta,
        TestEdge::Labeled,
        Some(PropertyValue::String("p0".into())),
    );
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let edge_id = collect_edges(&graph, alpha, TestEdge::Labeled, Direction::Out)
        .into_iter()
        .next()
        .expect("one edge");

    let labeled_edge = LabeledEdge::new(
        &graph,
        edge_id.src_node(),
        edge_id.dst_node(),
        edge_id.direction(),
        edge_id.seq(),
    );
    assert_eq!(labeled_edge.kind(), edge_id.kind());
    assert_eq!(labeled_edge.src_node().kind(), edge_id.src_node().kind());
    assert_eq!(labeled_edge.src_node().seq(), edge_id.src_node().seq());
    assert_eq!(labeled_edge.dst_node().kind(), edge_id.dst_node().kind());
    assert_eq!(labeled_edge.dst_node().seq(), edge_id.dst_node().seq());
    assert_eq!(labeled_edge.direction(), edge_id.direction());
    assert_eq!(labeled_edge.seq(), edge_id.seq());

    let prop = labeled_edge
        .property()
        .expect("edge property lookup")
        .expect("Labeled edges carry a property");
    assert_eq!(prop, "p0");

    let edge = Edge::new(
        &graph,
        TestEdge::Labeled,
        edge_id.src_node(),
        edge_id.dst_node(),
        edge_id.direction(),
        edge_id.seq(),
    );
    assert!(matches!(edge, Edge::Labeled(_)));
    assert_eq!(edge.kind(), edge_id.kind());
}

#[test]
fn in_edge_properties_match_their_edges() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha0 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let alpha1 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha0,
        beta,
        TestEdge::Labeled,
        Some(PropertyValue::String("p0".into())),
    );
    diff.add_edge(
        alpha1,
        beta,
        TestEdge::Labeled,
        Some(PropertyValue::String("p1".into())),
    );
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    let in_edges = collect_edges(&graph, beta, TestEdge::Labeled, Direction::In);
    assert_eq!(in_edges.len(), 2);

    for edge in in_edges {
        // Each A node carries the property named after its seq, so the edge
        // property must match the edge's source node.
        let expected = format!("p{}", edge.src_node().seq());
        let prop = graph
            .get_edge_property(edge)
            .expect("edge property lookup")
            .expect("property from In perspective");
        assert_eq!(string_value(&graph, prop), expected);
    }
}

#[test]
fn stored_node_edge_accessors_return_incident_edges() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha0 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let alpha1 = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha0,
        beta,
        TestEdge::Labeled,
        Some(PropertyValue::String("p0".into())),
    );
    diff.add_edge(
        alpha1,
        beta,
        TestEdge::Labeled,
        Some(PropertyValue::String("p1".into())),
    );
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let alpha0 = AlphaNode::new(&graph, 0);
    let beta = BetaNode::new(&graph, 0);

    let out_edges: Vec<_> = alpha0
        .get_edges_out(TestEdge::Labeled)
        .expect("alpha0 out edges")
        .collect();
    assert_eq!(out_edges.len(), 1);
    assert_eq!(out_edges[0].src_node().kind(), TestNode::Alpha);
    assert_eq!(out_edges[0].src_node().seq(), 0);
    assert_eq!(out_edges[0].dst_node().kind(), TestNode::Beta);
    assert_eq!(out_edges[0].dst_node().seq(), 0);

    let mut src_seqs: Vec<usize> = beta
        .get_edges_in(TestEdge::Labeled)
        .expect("beta in edges")
        .map(|edge| {
            assert_eq!(edge.src_node().kind(), TestNode::Alpha);
            edge.src_node().seq()
        })
        .collect();
    src_seqs.sort_unstable();
    assert_eq!(src_seqs, vec![0, 1]);

    assert_eq!(
        alpha0
            .get_edges_in(TestEdge::Labeled)
            .expect("alpha0 in edges")
            .len(),
        0
    );
    assert_eq!(
        beta.get_edges_out(TestEdge::Labeled)
            .expect("beta out edges")
            .len(),
        0
    );
}
