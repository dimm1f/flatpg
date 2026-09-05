use flatpg::{
    edge::{Direction, EdgeId},
    graph::{Graph, builder::GraphDiff},
    node::NodeId,
    prelude::*,
    storage::StoredProperty,
};
use test_fixtures::*;

pub fn string_value(graph: &Graph<TestSchema>, prop: StoredProperty) -> String {
    match prop {
        StoredProperty::StringId(v) => graph
            .resolve_string(v)
            .expect("string ref resolves")
            .to_string(),
        other => panic!("expected string property, got {other:?}"),
    }
}

pub fn setup_three_file_nodes() -> Graph<TestSchema> {
    let mut setup = GraphDiff::<TestSchema>::default();
    for name in ["a.rs", "b.rs", "c.rs"] {
        setup.add_node(
            builders::AlphaNodeBuilder::new()
                .add_property(TestProperty::Key, name.to_string())
                .unwrap()
                .build(),
        );
    }
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    graph
}

pub fn setup_graph_with_fan_out_edges() -> (
    Graph<TestSchema>,
    NodeId<TestSchema>,
    Vec<NodeId<TestSchema>>,
) {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_ids: Vec<_> = (0..3)
        .map(|_| setup.add_node(builders::BetaNodeBuilder::new().build()))
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
    let betas = graph.nodes_by_kind(TestNode::Beta).collect();
    (graph, alpha, betas)
}

pub fn collect_edges(
    graph: &Graph<TestSchema>,
    node: NodeId<TestSchema>,
    edge_kind: TestEdge,
    direction: Direction,
) -> Vec<EdgeId<TestSchema>> {
    graph
        .get_edges(node, edge_kind, direction)
        .expect("edge lookup")
        .collect()
}

pub fn out_edge_dst_seqs(graph: &Graph<TestSchema>, alpha: NodeId<TestSchema>) -> Vec<usize> {
    collect_edges(graph, alpha, TestEdge::Plain, Direction::Out)
        .iter()
        .map(|e| e.dst_node().seq())
        .collect()
}
