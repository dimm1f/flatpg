use flatpg::{
    error::Error,
    graph::{Graph, builder::GraphDiff},
    prelude::*,
};
use test_fixtures::*;

#[test]
fn add_single_node_to_empty_graph() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "main.rs".to_string())
            .unwrap()
            .build(),
    );

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

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

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

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

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

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

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
    assert_eq!(graph.node_count_by_kind(TestNode::Beta), 1);
    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "lib.rs");
    assert_eq!(BetaNode::new(&graph, 0).count().unwrap(), 7);
}

#[test]
fn add_node_without_properties() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(builders::AlphaNodeBuilder::new().build());

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

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
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "second.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
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
    let (graph, _) = diff
        .apply(TestGraphWrapper(Graph::new()))
        .expect("apply via GraphView wrapper");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
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

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

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

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(
        AlphaNode::new(&graph, 0).values().unwrap(),
        vec!["x0", "x1"]
    );
    assert!(AlphaNode::new(&graph, 1).values().unwrap().is_empty());
    assert_eq!(AlphaNode::new(&graph, 2).values().unwrap(), vec!["y0"]);
}

#[test]
fn add_node_without_property_then_node_with_property() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "b.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert!(matches!(
        AlphaNode::new(&graph, 0).key(),
        Err(Error::PropertyIndexNotFound)
    ));
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "b.rs");
}

#[test]
fn add_node_with_property_then_node_without_property() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "a.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "a.rs");
    assert!(matches!(
        AlphaNode::new(&graph, 1).key(),
        Err(Error::PropertyIndexNotFound)
    ));
}

#[test]
fn add_three_nodes_with_property_gap_in_middle() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "a.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "c.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff3.apply(graph).expect("apply diff 3");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "a.rs");
    assert!(matches!(
        AlphaNode::new(&graph, 1).key(),
        Err(Error::PropertyIndexNotFound)
    ));
    assert_eq!(AlphaNode::new(&graph, 2).key().unwrap(), "c.rs");
}

#[test]
fn add_three_nodes_without_property_gap_in_middle() {
    let mut diff1 = GraphDiff::<TestSchema>::default();
    diff1.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = diff1.apply(Graph::new()).expect("apply diff 1");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut diff2 = GraphDiff::<TestSchema>::default();
    diff2.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "b.rs".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff2.apply(graph).expect("apply diff 2");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut diff3 = GraphDiff::<TestSchema>::default();
    diff3.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = diff3.apply(graph).expect("apply diff 3");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert!(matches!(
        AlphaNode::new(&graph, 0).key(),
        Err(Error::PropertyIndexNotFound)
    ));
    assert_eq!(AlphaNode::new(&graph, 1).key().unwrap(), "b.rs");
    assert!(matches!(
        AlphaNode::new(&graph, 2).key(),
        Err(Error::PropertyIndexNotFound)
    ));
}
