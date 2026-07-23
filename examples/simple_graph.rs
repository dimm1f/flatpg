use flatpg::{
    edge::{Direction, StoredEdge},
    error::Error,
    graph::{Graph, GraphDiff},
    node::{NodeId, NodeRef},
    prelude::*,
    property::PropertyValue,
    schema::Schema,
};

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
#[edge_kind(schema = SimpleSchema)]
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

struct SimpleGraph<'a>(&'a Graph<SimpleSchema>);

impl<'a> SimpleGraph<'a> {
    fn new(graph: &'a Graph<SimpleSchema>) -> Self {
        Self(graph)
    }

    fn nodes_by_kind(&self, kind: SimpleNode) -> impl Iterator<Item = Node<'a>> + 'a {
        let graph = self.0;
        graph
            .nodes_by_kind(kind)
            .map(move |node| Node::new(graph, node.kind(), node.seq()))
    }

    fn nodes_by_kind_with_deleted(&self, kind: SimpleNode) -> impl Iterator<Item = Node<'a>> + 'a {
        let graph = self.0;
        graph
            .nodes_by_kind_with_deleted(kind)
            .map(move |node| Node::new(graph, node.kind(), node.seq()))
    }

    fn get_edges(
        &self,
        src_node: NodeId<SimpleSchema>,
        edge_kind: SimpleEdge,
        direction: Direction,
    ) -> Result<Vec<Edge<'a>>, Error> {
        let graph = self.0;
        Ok(graph
            .get_edges(src_node, edge_kind, direction)?
            .into_iter()
            .map(|edge| {
                Edge::new(
                    graph,
                    edge.kind(),
                    edge.src_node(),
                    edge.dst_node(),
                    edge.direction(),
                    edge.seq(),
                )
            })
            .collect())
    }
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

    let view = SimpleGraph::new(&graph);

    let Some(Node::A(a)) = view.nodes_by_kind(SimpleNode::A).next() else {
        panic!("expected Node::A");
    };
    assert_eq!(a.key().unwrap(), "hello");
    assert_eq!(a.values().unwrap(), vec!["v1", "v2"]);

    let Some(Node::B(b)) = view.nodes_by_kind(SimpleNode::B).next() else {
        panic!("expected Node::B");
    };
    assert_eq!(b.count().unwrap(), 42);

    let Some(Node::C(c)) = view.nodes_by_kind(SimpleNode::C).next() else {
        panic!("expected Node::C");
    };
    assert_eq!(c.r#ref().unwrap().kind(), SimpleNode::A);
    assert_eq!(c.r#ref().unwrap().seq(), a_node.seq());

    assert_eq!(view.nodes_by_kind_with_deleted(SimpleNode::A).count(), 1);

    let base_edges = view
        .get_edges(a_node, SimpleEdge::Base, Direction::Out)
        .expect("a's outgoing Base Edges");
    let [Edge::Base(base)] = base_edges.as_slice() else {
        panic!("expected exactly one Edge::Base");
    };
    assert_eq!(base.kind(), SimpleEdge::Base);
    assert_eq!(base.src_node().kind(), SimpleNode::A);
    assert_eq!(base.src_node().seq(), a_node.seq());
    assert_eq!(base.dst_node().kind(), SimpleNode::B);
    assert_eq!(base.dst_node().seq(), b_node.seq());
    assert_eq!(base.direction(), Direction::Out);

    let extended_edges = view
        .get_edges(a_node, SimpleEdge::Extended, Direction::In)
        .expect("a's incoming Extended Edges");
    let [Edge::Extended(extended)] = extended_edges.as_slice() else {
        panic!("expected exactly one Edge::Extended");
    };
    assert_eq!(extended.src_node().kind(), SimpleNode::C);
    let edge_property = extended
        .property()
        .expect("edge property lookup")
        .expect("Extended edges carry a property");
    assert_eq!(edge_property, "refers-to");

    println!("simple_graph example OK");
}
