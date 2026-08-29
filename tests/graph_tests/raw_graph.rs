use flatpg::{
    error::Error,
    graph::{Graph, builder::GraphDiff, integrity::CheckIntegrity, raw::RawGraph},
    schema::Schema,
    storage::{EdgeStorage, NodeMetaStorage, PropertyStorage},
};
use test_fixtures::*;

#[test]
fn raw_graph_default_and_new_are_both_empty_but_valid() {
    RawGraph::<TestSchema>::default()
        .check_integrity()
        .expect("default raw graph passes integrity check");
    RawGraph::<TestSchema>::new()
        .check_integrity()
        .expect("new raw graph passes integrity check");
}

#[test]
fn raw_graph_round_trips_a_graph_through_from_and_try_from() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");

    let raw: RawGraph<TestSchema> = graph.into();
    let graph: Graph<TestSchema> = raw.try_into().expect("valid raw graph converts back");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 1);
}

#[test]
fn raw_graph_with_extra_node_meta_slot_fails_conversion_back_to_graph() {
    let mut raw = RawGraph::<TestSchema>::default();
    raw.node_meta_storage.push(Vec::new());

    let err = Graph::try_from(raw).err().expect("expected an error");
    assert!(matches!(err, Error::StorageSizeMismatch { .. }));
}

#[test]
fn raw_graph_with_extra_property_storage_slot_fails_check_integrity() {
    let mut raw = RawGraph::<TestSchema>::default();
    raw.property_storage.push(Default::default());

    let err = raw.check_integrity().unwrap_err();
    assert!(matches!(err, Error::StorageSizeMismatch { .. }));
}

#[test]
fn storage_container_defaults_are_sized_from_the_schema() {
    let node_meta = NodeMetaStorage::<TestSchema>::default();
    assert_eq!(node_meta.len(), TestSchema::number_of_node_kinds());

    let edge_storage = EdgeStorage::<TestSchema>::default();
    assert_eq!(edge_storage.len(), TestSchema::edge_storage_size());
    assert_eq!((&edge_storage).into_iter().count(), edge_storage.len());

    let property_storage = PropertyStorage::<TestSchema>::default();
    assert_eq!(property_storage.len(), TestSchema::property_storage_size());
    assert_eq!(
        (&property_storage).into_iter().count(),
        property_storage.len()
    );
}
