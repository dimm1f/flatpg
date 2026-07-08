use flatpg::{
    graph::{Graph, GraphDiff},
    node::NodeRef,
    prelude::*,
    property::PropertyValue,
    schema::Schema,
};
use graph_schema::edge::Direction;

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, PropertyItemKind)]
enum SimpleProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = NodeRef, quantity = One)]
    Ref,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = SimpleSchema, property_kind = SimpleProperty)]
enum SimpleNode {
    #[properties(Key, Values)]
    A,
    #[properties(Count)]
    B,
    #[properties(Ref)]
    C,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, EdgeItemKind)]
enum SimpleEdge {
    #[property(typ = None)]
    Base,
    #[property(typ = String)]
    Extended,
}

#[derive(Clone, Copy, Default)]
struct SimpleSchema;

impl Schema for SimpleSchema {
    type N = SimpleNode;
    type E = SimpleEdge;
    type P = SimpleProperty;
}

fn main() {
    let mut diff = GraphDiff::<SimpleSchema>::default();
    let a_id = diff.add_node(
        builders::ANodeBuilder::new()
            .add_property(SimpleProperty::Key, "hello".to_string())
            .unwrap()
            .add_property(SimpleProperty::Values, "v1".to_string())
            .unwrap()
            .add_property(SimpleProperty::Values, "v2".to_string())
            .unwrap()
            .build(),
    );
    let b_id = diff.add_node(
        builders::BNodeBuilder::new()
            .add_property(SimpleProperty::Count, 42i32)
            .unwrap()
            .build(),
    );
    diff.add_edge(a_id, b_id, SimpleEdge::Base, None);

    let graph = diff
        .apply(Graph::<SimpleSchema>::new())
        .expect("apply diff 1");

    let a_node = graph.nodes_by_kind(SimpleNode::A).next().expect("A node");
    let b_node = graph.nodes_by_kind(SimpleNode::B).next().expect("B node");
    let a_ref = NodeRef::from(&a_node);

    let mut diff = GraphDiff::<SimpleSchema>::default();
    let c_id = diff.add_node(
        builders::CNodeBuilder::new()
            .add_property(SimpleProperty::Ref, a_ref)
            .unwrap()
            .build(),
    );
    // Edges can also connect a new node to one already in the graph, and
    // carry a property (SimpleEdge::Extended has `typ = String`).
    diff.add_edge(
        c_id,
        a_ref,
        SimpleEdge::Extended,
        Some(PropertyValue::String("refers-to".to_string())),
    );

    let graph = diff.apply(graph).expect("apply diff 2");

    let c_node = graph.nodes_by_kind(SimpleNode::C).next().expect("C node");

    let a = ANode::new(&graph, a_node.seq());
    let b = BNode::new(&graph, b_node.seq());
    let c = CNode::new(&graph, c_node.seq());

    assert_eq!(a.key().unwrap(), "hello");
    assert_eq!(a.values().unwrap(), vec!["v1", "v2"]);
    assert_eq!(b.count().unwrap(), 42);
    assert_eq!(c.r#ref().unwrap().kind(), SimpleNode::A);
    assert_eq!(c.r#ref().unwrap().seq(), a_node.seq());

    let a_out_edges = graph
        .get_edges(a_node, SimpleEdge::Base, Direction::Out)
        .expect("a's outgoing Base edges");
    assert_eq!(a_out_edges.len(), 1);
    assert_eq!(a_out_edges[0].dst_node().kind(), SimpleNode::B);
    assert_eq!(a_out_edges[0].dst_node().seq(), b_node.seq());

    let a_in_edges = graph
        .get_edges(a_node, SimpleEdge::Extended, Direction::In)
        .expect("a's incoming Extended edges");
    assert_eq!(a_in_edges.len(), 1);
    assert_eq!(a_in_edges[0].src_node().kind(), SimpleNode::C);
    let edge_property = graph
        .get_edge_property(a_in_edges.into_iter().next().unwrap())
        .expect("edge property lookup")
        .expect("Extended edges carry a property");
    match edge_property {
        PropertyValue::String(s) => assert_eq!(s, "refers-to"),
        other => panic!("expected string property, got {other:?}"),
    }

    println!("simple_graph example OK");
}
