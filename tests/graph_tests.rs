use flatpg::{
    edge::{Direction, StoredEdge},
    error::Error,
    graph::{Graph, GraphDiff, QuantifiedProperty},
    node::{NodeId, NodeRef, StoredNode},
    prelude::*,
    property::PropertyValue,
    schema::Schema,
    storage::StoredProperty,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumProperty)]
enum Status {
    Active,
    Inactive,
    Banned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumPropertyRegistry)]
enum TestPropEnumsRegistry {
    #[enum_type(Status)]
    Status,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, PropertyItemKind)]
enum TestProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = Enum<Status>, quantity = One)]
    State,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = TestSchema, property_kind = TestProperty)]
enum TestNode {
    #[properties(Key, Values, State)]
    Alpha,
    #[properties(Count)]
    Beta,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, EdgeItemKind)]
#[edge_kind(schema = TestSchema)]
enum TestEdge {
    #[property(typ = None)]
    Plain,
    #[property(typ = String)]
    Labeled,
}

#[derive(Clone, Copy, Default)]
struct TestSchema;

impl Schema for TestSchema {
    type N = TestNode;
    type E = TestEdge;
    type P = TestProperty;
    type EPR = TestPropEnumsRegistry;
}

fn string_value(graph: &Graph<TestSchema>, prop: StoredProperty) -> String {
    match prop {
        StoredProperty::StringRef(v) => graph
            .resolve_string(v)
            .expect("string ref resolves")
            .to_string(),
        other => panic!("expected string property, got {other:?}"),
    }
}

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
    let graph = diff.apply(Graph::new()).expect("apply diff");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");

    let mut out_edges = graph
        .get_edges(alpha, TestEdge::Labeled, Direction::Out)
        .expect("out edges");
    assert_eq!(out_edges.len(), 1);
    let out_prop = graph
        .get_edge_property(out_edges.remove(0))
        .expect("edge property lookup")
        .expect("property from Out perspective");
    assert_eq!(string_value(&graph, out_prop), "p0");

    let mut in_edges = graph
        .get_edges(beta, TestEdge::Labeled, Direction::In)
        .expect("in edges");
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
    let graph = diff.apply(Graph::new()).expect("apply diff");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let edge_id = graph
        .get_edges(alpha, TestEdge::Labeled, Direction::Out)
        .expect("out edges")
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
    let graph = diff.apply(Graph::new()).expect("apply diff");

    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    let in_edges = graph
        .get_edges(beta, TestEdge::Labeled, Direction::In)
        .expect("in edges");
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
    let graph = diff.apply(Graph::new()).expect("apply diff");

    let alpha0 = AlphaNode::new(&graph, 0);
    let beta = BetaNode::new(&graph, 0);

    let out_edges = alpha0
        .get_edges_out(TestEdge::Labeled)
        .expect("alpha0 out edges");
    assert_eq!(out_edges.len(), 1);
    assert_eq!(out_edges[0].src_node().kind(), TestNode::Alpha);
    assert_eq!(out_edges[0].src_node().seq(), 0);
    assert_eq!(out_edges[0].dst_node().kind(), TestNode::Beta);
    assert_eq!(out_edges[0].dst_node().seq(), 0);

    let in_edges = beta.get_edges_in(TestEdge::Labeled).expect("beta in edges");
    let mut src_seqs: Vec<usize> = in_edges
        .iter()
        .map(|edge| {
            assert_eq!(edge.src_node().kind(), TestNode::Alpha);
            edge.src_node().seq()
        })
        .collect();
    src_seqs.sort_unstable();
    assert_eq!(src_seqs, vec![0, 1]);

    // The opposite halves carry no edges.
    assert!(
        alpha0
            .get_edges_in(TestEdge::Labeled)
            .expect("alpha0 in edges")
            .is_empty()
    );
    assert!(
        beta.get_edges_out(TestEdge::Labeled)
            .expect("beta out edges")
            .is_empty()
    );
}

#[test]
fn add_property_rejects_unsupported_property() {
    let result = builders::BetaNodeBuilder::new().add_property(TestProperty::Key, "x".to_string());
    assert!(matches!(result, Err(Error::PropertyNotSupported { .. })));
}

#[test]
fn add_property_rejects_second_value_for_quantity_one() {
    let result = builders::AlphaNodeBuilder::new()
        .add_property(TestProperty::Key, "first".to_string())
        .expect("first Key value is accepted")
        .add_property(TestProperty::Key, "second".to_string());
    assert!(matches!(result, Err(Error::PropertyAlreadySet(_))));
}

#[test]
fn add_property_allows_multiple_values_for_quantity_multi() {
    builders::AlphaNodeBuilder::new()
        .add_property(TestProperty::Values, "v1".to_string())
        .expect("first Multi value is accepted")
        .add_property(TestProperty::Values, "v2".to_string())
        .expect("second Multi value is accepted");
}

#[test]
fn add_single_node_to_empty_graph() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "main.rs".to_string())
            .unwrap()
            .build(),
    );

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "main.rs");
}

#[test]
fn add_node_with_enum_property_round_trips() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "main.rs".to_string())
            .unwrap()
            .add_property(TestProperty::State, Status::Banned)
            .unwrap()
            .build(),
    );

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(AlphaNode::new(&graph, 0).state().unwrap(), Status::Banned);
}

#[test]
fn add_multiple_nodes_same_kind_preserves_order() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "a.rs".to_string())
            .unwrap()
            .build(),
    );
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "b.rs".to_string())
            .unwrap()
            .build(),
    );

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 2);
    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "a.rs");
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "b.rs");
}

#[test]
fn add_nodes_of_different_kinds() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "lib.rs".to_string())
            .unwrap()
            .build(),
    );
    diff.add_node(
        builders::BetaNodeBuilder::new()
            .add_property(TestProperty::Count, 7i32)
            .unwrap()
            .build(),
    );

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(graph.node_count_by_kind(TestNode::Beta), 1);
    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "lib.rs");
    assert_eq!(BetaNode::new(&graph, 0).count().unwrap(), 7);
}

#[test]
fn add_node_without_properties() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(builders::AlphaNodeBuilder::new().build());

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
}

#[test]
fn apply_incremental_to_existing_graph() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "first.rs".to_string())
            .unwrap()
            .build(),
    );
    let graph = diff1.apply(Graph::new()).expect("apply diff 1");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "second.rs".to_string())
            .unwrap()
            .build(),
    );
    let graph = diff2.apply(graph).expect("apply diff 2");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 2);

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "first.rs");
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "second.rs");
}

#[test]
fn apply_accepts_any_graph_view_implementor() {
    struct TestGraphWrapper(Graph<TestSchema>);

    impl GraphView<TestSchema> for TestGraphWrapper {
        fn graph(&self) -> &Graph<TestSchema> {
            &self.0
        }

        fn into_graph(self) -> Graph<TestSchema> {
            self.0
        }
    }

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "wrapped.rs".to_string())
            .unwrap()
            .build(),
    );
    let graph = diff
        .apply(TestGraphWrapper(Graph::new()))
        .expect("apply via GraphView wrapper");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "wrapped.rs");
}

#[test]
fn add_node_with_multi_valued_property_stores_all_values() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "v1".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "v2".to_string())
            .unwrap()
            .build(),
    );

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(
        AlphaNode::new(&graph, 0).values().unwrap(),
        vec!["v1", "v2"]
    );
}

#[test]
fn multi_valued_property_offsets_are_correct_across_nodes() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "x0".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "x1".to_string())
            .unwrap()
            .build(),
    );
    diff.add_node(builders::AlphaNodeBuilder::new().build());
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "y0".to_string())
            .unwrap()
            .build(),
    );

    let graph = diff.apply(Graph::new()).expect("apply diff");

    assert_eq!(
        AlphaNode::new(&graph, 0).values().unwrap(),
        vec!["x0", "x1"]
    );
    assert!(AlphaNode::new(&graph, 1).values().unwrap().is_empty());
    assert_eq!(AlphaNode::new(&graph, 2).values().unwrap(), vec!["y0"]);
}

#[test]
fn add_edge_between_new_nodes() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_id, beta_id, TestEdge::Plain, None);

    let graph = diff.apply(Graph::new()).expect("apply diff");

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
            .get_edges_count(NodeRef::from(&alpha), TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta), TestEdge::Plain, Direction::In)
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

    let graph = diff.apply(Graph::new()).expect("apply diff");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");

    let out_edges = graph
        .get_edges(alpha, TestEdge::Plain, Direction::Out)
        .expect("out edges");
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

    let graph = diff.apply(Graph::new()).expect("apply diff");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&alpha), TestEdge::Plain, Direction::Out)
            .expect("out edges count"),
        2
    );
}

#[test]
fn add_edge_between_existing_nodes() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    setup.add_node(builders::BetaNodeBuilder::new().build());
    let graph = setup.apply(Graph::new()).expect("apply setup");

    let alpha_ref = NodeRef::from(
        &graph
            .nodes_by_kind(TestNode::Alpha)
            .next()
            .expect("Alpha node"),
    );
    let beta_ref = NodeRef::from(
        &graph
            .nodes_by_kind(TestNode::Beta)
            .next()
            .expect("Beta node"),
    );

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_edge(alpha_ref, beta_ref, TestEdge::Plain, None);
    let graph = diff.apply(graph).expect("apply diff");

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
    let graph = setup.apply(Graph::new()).expect("apply setup");

    let alpha_ref = NodeRef::from(
        &graph
            .nodes_by_kind(TestNode::Alpha)
            .next()
            .expect("Alpha node"),
    );

    let mut diff = GraphDiff::<TestSchema>::default();
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(alpha_ref, beta_id, TestEdge::Plain, None);
    let graph = diff.apply(graph).expect("apply diff");

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
            .get_edges_count(NodeRef::from(&beta), TestEdge::Plain, Direction::In)
            .expect("in edges count"),
        1
    );
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

    let graph = diff.apply(Graph::new()).expect("apply diff");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let edges = graph
        .get_edges(alpha, TestEdge::Labeled, Direction::Out)
        .expect("out edges");
    assert_eq!(edges.len(), 1);

    let property = graph
        .get_edge_property(edges.into_iter().next().unwrap())
        .expect("edge property lookup")
        .expect("edge property should be set");
    assert_eq!(string_value(&graph, property), "x");
}

fn setup_three_file_nodes() -> Graph<TestSchema> {
    let mut setup = GraphDiff::<TestSchema>::default();
    for name in ["a.rs", "b.rs", "c.rs"] {
        setup.add_node(
            builders::AlphaNodeBuilder::new()
                .add_property(TestProperty::Key, name.to_string())
                .unwrap()
                .build(),
        );
    }
    setup.apply(Graph::new()).expect("apply setup")
}

#[test]
fn remove_first_of_many_nodes_preserves_others() {
    let graph = setup_three_file_nodes();
    let nodes: Vec<NodeId<TestSchema>> = graph.nodes_by_kind(TestNode::Alpha).collect();

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(&nodes[0]);
    let graph = diff.apply(graph).expect("apply diff");

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
    let graph = diff.apply(graph).expect("apply diff");

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
    let graph = diff.apply(graph).expect("apply diff");

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

fn setup_graph_with_fan_out_edges() -> (
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
    let graph = setup.apply(Graph::new()).expect("apply setup");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let betas = graph.nodes_by_kind(TestNode::Beta).collect();
    (graph, alpha, betas)
}

fn out_edge_dst_seqs(graph: &Graph<TestSchema>, alpha: NodeId<TestSchema>) -> Vec<usize> {
    graph
        .get_edges(alpha, TestEdge::Plain, Direction::Out)
        .expect("out edges")
        .iter()
        .map(|e| e.dst_node().seq())
        .collect()
}

#[test]
fn remove_first_of_many_out_edges_preserves_others() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b0 = graph
        .get_edges(alpha, TestEdge::Plain, Direction::Out)
        .expect("out edges")
        .into_iter()
        .find(|e| e.dst_node().seq() == beta0.seq())
        .expect("edge to beta0");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b0);
    let graph = diff.apply(graph).expect("apply diff");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&beta1.seq()));
    assert!(dsts.contains(&beta2.seq()));
    assert!(!dsts.contains(&beta0.seq()));

    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta0), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta1), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta2), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
}

#[test]
fn remove_middle_of_many_out_edges_preserves_others() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b1 = graph
        .get_edges(alpha, TestEdge::Plain, Direction::Out)
        .expect("out edges")
        .into_iter()
        .find(|e| e.dst_node().seq() == beta1.seq())
        .expect("edge to beta1");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b1);
    let graph = diff.apply(graph).expect("apply diff");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&beta0.seq()));
    assert!(dsts.contains(&beta2.seq()));
    assert!(!dsts.contains(&beta1.seq()));

    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta1), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}

#[test]
fn remove_last_of_many_out_edges_preserves_others() {
    let (graph, alpha, betas) = setup_graph_with_fan_out_edges();
    let (beta0, beta1, beta2) = (betas[0], betas[1], betas[2]);

    let edge_to_b2 = graph
        .get_edges(alpha, TestEdge::Plain, Direction::Out)
        .expect("out edges")
        .into_iter()
        .find(|e| e.dst_node().seq() == beta2.seq())
        .expect("edge to beta2");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_edge(edge_to_b2);
    let graph = diff.apply(graph).expect("apply diff");

    let dsts = out_edge_dst_seqs(&graph, alpha);
    assert_eq!(dsts.len(), 2);
    assert!(dsts.contains(&beta0.seq()));
    assert!(dsts.contains(&beta1.seq()));
    assert!(!dsts.contains(&beta2.seq()));

    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta2), TestEdge::Plain, Direction::In)
            .unwrap(),
        0
    );
}

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
    let graph = diff.apply(graph).expect("apply diff");

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
    let graph = diff.apply(graph).expect("apply diff");

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
    let graph = diff.apply(graph).expect("apply diff");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "a.rs");
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "b.rs");
    assert_eq!(AlphaNode::new(&graph, 2).key().unwrap(), "updated.rs");
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
    let graph = diff1.apply(Graph::new()).expect("apply diff 1");

    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.remove_node(&node);
    let graph = diff2.apply(graph).expect("apply diff 2");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 0);

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "second.rs".to_string())
            .unwrap()
            .build(),
    );
    let graph = diff3.apply(graph).expect("apply diff 3");

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
    let graph = diff1.apply(Graph::new()).expect("apply diff 1");
    let node = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.update_node_property(&node, TestProperty::Key, QuantifiedProperty::Multi(vec![]));
    let graph = diff2.apply(graph).expect("apply diff 2");
    assert_eq!(
        graph
            .get_node_property(NodeRef::from(&node), TestProperty::Key)
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
    let graph = diff3.apply(graph).expect("apply diff 3");

    assert_eq!(
        AlphaNode::new(&graph, node.seq()).key().unwrap(),
        "restored.rs"
    );
}

#[test]
fn add_edge_remove_then_readd_edge_is_accessible() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    let graph = setup.apply(Graph::new()).expect("apply setup");

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");

    let edges = graph
        .get_edges(alpha, TestEdge::Plain, Direction::Out)
        .expect("out edges");
    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.remove_edge(edges.into_iter().next().unwrap());
    let graph = diff2.apply(graph).expect("apply diff 2");
    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&alpha), TestEdge::Plain, Direction::Out)
            .unwrap(),
        0
    );

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.add_edge(
        NodeRef::from(&alpha),
        NodeRef::from(&beta),
        TestEdge::Plain,
        None,
    );
    let graph = diff3.apply(graph).expect("apply diff 3");

    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&alpha), TestEdge::Plain, Direction::Out)
            .unwrap(),
        1
    );
    assert_eq!(
        graph
            .get_edges_count(NodeRef::from(&beta), TestEdge::Plain, Direction::In)
            .unwrap(),
        1
    );
}
