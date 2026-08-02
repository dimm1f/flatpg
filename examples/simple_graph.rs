use flatpg::{
    edge::{Direction, StoredEdge},
    graph::{Graph, builder::GraphDiff},
    prelude::*,
    property::PropertyValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumProperty)]
enum Status {
    Active,
    Inactive,
    Banned,
}

enum_property_registry!(SimplePropEnumsRegistry: Status);

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, PropertyItemKind)]
enum SimpleProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = NodeId, quantity = One)]
    Ref,
    #[property(typ = Enum<Status>, quantity = One)]
    State,
    #[property(typ = String, quantity = One, rename = Label)]
    Tag,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = SimpleSchema, property_kind = SimpleProperty)]
enum SimpleNode {
    #[properties(Key, Values)]
    Alpha,
    #[properties(Count)]
    Beta,
    #[properties(Ref, State)]
    Gamma,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, EdgeItemKind)]
#[edge_kind(schema = SimpleSchema)]
enum SimpleEdge {
    #[property(typ = None)]
    Base,
    #[property(typ = String)]
    Extended,
}

schema!(SimpleSchema: SimpleNode, SimpleEdge, SimpleProperty, SimplePropEnumsRegistry);

fn main() {
    let mut diff = GraphDiff::<SimpleSchema>::default();
    let alpha_id = diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(SimpleProperty::Key, "hello".to_string())
            .unwrap()
            .add_property(SimpleProperty::Values, "v1".to_string())
            .unwrap()
            .add_property(SimpleProperty::Values, "v2".to_string())
            .unwrap()
            .build(),
    );
    let beta_id = diff.add_node(
        builders::BetaNodeBuilder::new()
            .add_property(SimpleProperty::Count, 42i32)
            .unwrap()
            .build(),
    );
    diff.add_edge(alpha_id, beta_id, SimpleEdge::Base, None);

    let graph = diff
        .apply(Graph::<SimpleSchema>::new())
        .expect("apply diff 1");

    let alpha_node = graph
        .nodes_by_kind(SimpleNode::Alpha)
        .next()
        .expect("Alpha node");
    let beta_node = graph
        .nodes_by_kind(SimpleNode::Beta)
        .next()
        .expect("Beta node");

    let mut diff = GraphDiff::<SimpleSchema>::default();
    let gamma_id = diff.add_node(
        builders::GammaNodeBuilder::new()
            .add_property(SimpleProperty::Ref, alpha_node)
            .unwrap()
            .add_property(SimpleProperty::State, Status::Active)
            .unwrap()
            .build(),
    );
    // Edges can also connect a new node to one already in the graph, and
    // carry a property (SimpleEdge::Extended has `typ = String`).
    diff.add_edge(
        gamma_id,
        alpha_node,
        SimpleEdge::Extended,
        Some(PropertyValue::String("refers-to".to_string())),
    );

    let graph = diff.apply(graph).expect("apply diff 2");

    let Some(Node::Alpha(alpha)) = graph.alpha().next() else {
        panic!("expected Node::Alpha");
    };
    assert_eq!(alpha.key().unwrap(), "hello");
    assert_eq!(alpha.values().unwrap(), vec!["v1", "v2"]);

    let Some(Node::Beta(beta)) = graph.beta().next() else {
        panic!("expected Node::Beta");
    };
    assert_eq!(beta.count().unwrap(), 42);

    let Some(Node::Gamma(gamma)) = graph.gamma().next() else {
        panic!("expected Node::Gamma");
    };
    // `ref` is a Rust keyword, so the generated accessor for the `Ref` property
    // is escaped as a raw identifier.
    assert_eq!(gamma.r#ref().unwrap().kind(), SimpleNode::Alpha);
    assert_eq!(gamma.r#ref().unwrap().seq(), alpha_node.seq());
    assert_eq!(gamma.state().unwrap(), Status::Active);

    assert_eq!(
        graph.nodes_by_kind_with_deleted(SimpleNode::Alpha).count(),
        1
    );

    let base_edges = graph
        .edges(alpha_node, SimpleEdge::Base, Direction::Out)
        .expect("alpha's outgoing Base Edges");
    let [Edge::Base(base)] = base_edges.as_slice() else {
        panic!("expected exactly one Edge::Base");
    };
    assert_eq!(base.kind(), SimpleEdge::Base);
    assert_eq!(base.src_node().kind(), SimpleNode::Alpha);
    assert_eq!(base.src_node().seq(), alpha_node.seq());
    assert_eq!(base.dst_node().kind(), SimpleNode::Beta);
    assert_eq!(base.dst_node().seq(), beta_node.seq());
    assert_eq!(base.direction(), Direction::Out);

    let extended_edges = graph
        .edges(alpha_node, SimpleEdge::Extended, Direction::In)
        .expect("alpha's incoming Extended Edges");
    let [Edge::Extended(extended)] = extended_edges.as_slice() else {
        panic!("expected exactly one Edge::Extended");
    };
    assert_eq!(extended.src_node().kind(), SimpleNode::Gamma);
    let edge_property = extended
        .property()
        .expect("edge property lookup")
        .expect("Extended edges carry a property");
    assert_eq!(edge_property, "refers-to");

    assert_eq!(SimpleProperty::Tag.as_str(), "Label");
    assert_eq!(
        "Label".parse::<SimpleProperty>().unwrap(),
        SimpleProperty::Tag
    );
    assert!("Tag".parse::<SimpleProperty>().is_err());

    println!("simple_graph example OK");
}
