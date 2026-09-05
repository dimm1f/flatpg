use flatpg::{
    error::Error,
    graph::{
        Graph,
        builder::{GraphDiff, QuantifiedProperty},
        raw::RawGraph,
    },
    node::{NewNode, NodeId},
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::setup_three_file_nodes;

fn alpha_node(key: &str) -> NewNode<TestSchema> {
    builders::AlphaNodeBuilder::new()
        .add_property(TestProperty::Key, key.to_string())
        .expect("Key is accepted")
        .build()
}

fn alpha_keys(graph: &Graph<TestSchema>) -> Vec<String> {
    graph
        .nodes_by_kind(TestNode::Alpha)
        .map(|node| {
            AlphaNode::new(graph, node.seq())
                .key()
                .expect("Alpha key")
                .to_string()
        })
        .collect()
}

fn strings_count(graph: Graph<TestSchema>) -> usize {
    RawGraph::from(graph).strings.len()
}

#[test]
fn dropping_a_staged_diff_leaves_the_graph_unchanged() {
    let mut graph = setup_three_file_nodes();
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(alpha_node("d.rs"));

    let staged = diff.prepare(&mut graph).expect("prepare diff");
    drop(staged);

    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 3);
    assert_eq!(alpha_keys(&graph), ["a.rs", "b.rs", "c.rs"]);
    assert_eq!(strings_count(graph), 3);
}

#[test]
fn commit_applies_the_staged_changes() {
    let mut graph = setup_three_file_nodes();
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(alpha_node("d.rs"));

    let new_ids = diff.prepare(&mut graph).expect("prepare diff").commit();

    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(new_ids.iter().map(NodeId::seq).collect::<Vec<_>>(), vec![3]);
    assert_eq!(alpha_keys(&graph), ["a.rs", "b.rs", "c.rs", "d.rs"]);
    assert_eq!(strings_count(graph), 4);
}

/// Locks the string pool against a failed `prepare`: the diff's `"d.rs"` and `"x"` are both
/// resolved to string ids before the edge check rejects it, so leaking them into the pool
/// would leave the graph holding strings no property refers to.
#[test]
fn failed_prepare_leaves_the_graph_unchanged() {
    let mut graph = setup_three_file_nodes();
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha = diff.add_node(alpha_node("d.rs"));
    let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha,
        beta,
        TestEdge::Plain,
        Some(PropertyValue::String("x".to_string())),
    );

    let err = diff.prepare(&mut graph).err().expect("expected an error");

    assert!(matches!(err, Error::InvalidPropertyType { .. }));
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 3);
    assert_eq!(alpha_keys(&graph), ["a.rs", "b.rs", "c.rs"]);
    assert_eq!(strings_count(graph), 3);
}

#[test]
fn staged_strings_are_deduplicated_against_the_pool() {
    let mut graph = setup_three_file_nodes();
    let mut diff = GraphDiff::<TestSchema>::default();
    for key in ["a.rs", "d.rs", "d.rs"] {
        diff.add_node(alpha_node(key));
    }

    diff.prepare(&mut graph).expect("prepare diff").commit();

    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(
        alpha_keys(&graph),
        ["a.rs", "b.rs", "c.rs", "a.rs", "d.rs", "d.rs"]
    );
    assert_eq!(strings_count(graph), 4);
}

#[test]
fn prepare_accepts_any_graph_view_mut_implementor() {
    struct TestGraphWrapper(Graph<TestSchema>);

    impl GraphView<TestSchema> for TestGraphWrapper {
        fn graph(&self) -> &Graph<TestSchema> {
            &self.0
        }

        fn into_graph(self) -> Graph<TestSchema> {
            self.0
        }
    }

    impl GraphViewMut<TestSchema> for TestGraphWrapper {
        fn graph_mut(&mut self) -> &mut Graph<TestSchema> {
            &mut self.0
        }
    }

    let mut wrapper = TestGraphWrapper(setup_three_file_nodes());
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(alpha_node("d.rs"));

    diff.prepare(&mut wrapper).expect("prepare diff").commit();

    let graph = wrapper.into_graph();
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(alpha_keys(&graph), ["a.rs", "b.rs", "c.rs", "d.rs"]);
}

#[test]
fn prepare_then_commit_matches_apply() {
    fn mixed_diff(existing: &[NodeId<TestSchema>]) -> GraphDiff<TestSchema> {
        let mut diff = GraphDiff::<TestSchema>::default();
        let alpha = diff.add_node(alpha_node("d.rs"));
        let beta = diff.add_node(builders::BetaNodeBuilder::new().build());
        diff.add_edge(alpha, beta, TestEdge::Plain, None);
        diff.add_edge(
            existing[0],
            beta,
            TestEdge::Labeled,
            Some(PropertyValue::String("edge label".to_string())),
        );
        diff.update_node_property(
            &existing[1],
            TestProperty::Key,
            QuantifiedProperty::One(PropertyValue::String("updated.rs".to_string())),
        );
        diff.remove_node(&existing[2]);
        diff
    }

    let applied = setup_three_file_nodes();
    let existing: Vec<NodeId<TestSchema>> = applied.nodes_by_kind(TestNode::Alpha).collect();
    let (applied, applied_ids) = mixed_diff(&existing).apply(applied).expect("apply diff");

    let mut staged = setup_three_file_nodes();
    let existing: Vec<NodeId<TestSchema>> = staged.nodes_by_kind(TestNode::Alpha).collect();
    let staged_ids = mixed_diff(&existing)
        .prepare(&mut staged)
        .expect("prepare diff")
        .commit();

    staged
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(
        staged_ids.iter().map(NodeId::seq).collect::<Vec<_>>(),
        applied_ids.iter().map(NodeId::seq).collect::<Vec<_>>()
    );
    assert_eq!(alpha_keys(&staged), alpha_keys(&applied));
    assert_eq!(
        staged.node_count_by_kind(TestNode::Beta),
        applied.node_count_by_kind(TestNode::Beta)
    );
    assert_eq!(strings_count(staged), strings_count(applied));
}
