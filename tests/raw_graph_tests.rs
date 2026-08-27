use flatpg::{
    edge::Direction,
    error::Error,
    graph::{Graph, builder::GraphDiff, raw::RawGraph},
    prelude::*,
    property::PropertyValue,
    schema::Schema,
    storage::{Offset, OffsetStorage, StorageArray},
    strings_pool::RawStringId,
};
use test_fixtures::*;

/// Corrupts the graph on purpose, for testing.
///
/// Removes the last node of `kind` and fixes `kind`'s own storage (offsets and values)
/// so it stays consistent with the new, smaller node count.
///
/// It does NOT fix edges from *other* node kinds that still point to the removed node.
/// This leaves a dangling `RawNodeId` in the graph, so tests can check that this bad
/// reference is detected and rejected.
fn shrink_node_count<S: Schema>(raw: &mut RawGraph<S>, kind: S::N) {
    raw.node_meta_storage[kind.index()].pop();

    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        if node_kind != kind {
            continue;
        }
        let slot_index = S::property_storage_slot(node_kind, property_kind).index();
        let slot = &mut raw.property_storage[slot_index];
        if slot.offsets().is_empty() {
            continue;
        }
        let removed_end = slot.offsets_mut().pop().unwrap();
        let new_end = *slot.offsets().last().unwrap();
        let range = new_end.value()..removed_end.value();
        slot.values_mut().try_drain(range).unwrap();
    }

    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        if node_kind != kind {
            continue;
        }
        let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind).index();
        let slot = &mut raw.edge_storage[slot_index];
        if slot.offsets().is_empty() {
            continue;
        }
        let removed_end = slot.offsets_mut().pop().unwrap();
        let new_end = *slot.offsets().last().unwrap();
        let range = new_end.value()..removed_end.value();
        slot.neighbors_mut().drain(range.clone());
        slot.values_mut().try_drain(range).unwrap();
    }
}

fn build_two_alpha_values_graph() -> Graph<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(builders::AlphaNodeBuilder::new().build());
    diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Values, "v0".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "v1".to_string())
            .unwrap()
            .build(),
    );
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    graph
}

#[test]
fn empty_graph_round_trips_through_raw_graph() {
    let raw: RawGraph<TestSchema> = Graph::<TestSchema>::new().into();
    let graph: Graph<TestSchema> = raw.try_into().expect("empty graph is valid");
    assert_eq!(graph.node_count_by_kind(TestNode::Alpha), 0);
    assert_eq!(graph.node_count_by_kind(TestNode::Beta), 0);
}

#[test]
fn populated_graph_round_trips_through_raw_graph() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "main.rs".to_string())
            .unwrap()
            .add_property(TestProperty::State, Status::Banned)
            .unwrap()
            .build(),
    );
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Labeled,
        Some(PropertyValue::String("p0".to_string())),
    );
    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let raw: RawGraph<TestSchema> = graph.into();
    let graph: Graph<TestSchema> = raw.try_into().expect("populated graph is valid");

    assert_eq!(AlphaNode::new(&graph, 0).key().unwrap(), "main.rs");
    assert_eq!(AlphaNode::new(&graph, 0).state().unwrap(), Status::Banned);

    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");
    let out_edges = graph
        .get_edges(alpha, TestEdge::Labeled, Direction::Out)
        .expect("out edges");
    assert_eq!(out_edges.len(), 1);
}

#[test]
fn edge_to_soft_deleted_node_passes_check_integrity() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let beta = graph
        .nodes_by_kind(TestNode::Beta)
        .next()
        .expect("Beta node");
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.remove_node(&beta);
    let (graph, _) = diff.apply(graph).expect("apply remove");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    assert!(graph.check_integrity().is_ok());
}

#[test]
fn storage_size_mismatch_is_rejected() {
    let mut raw: RawGraph<TestSchema> = Graph::<TestSchema>::new().into();
    raw.edge_storage.pop();

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::StorageSizeMismatch { .. }));
}

#[test]
fn non_monotonic_offsets_are_rejected() {
    let mut raw: RawGraph<TestSchema> = build_two_alpha_values_graph().into();
    let slot_index =
        TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Values).index();
    raw.property_storage[slot_index].offsets_mut().swap(1, 2);

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::OffsetUnderflow));
}

#[test]
fn offsets_length_mismatch_is_rejected() {
    let mut raw: RawGraph<TestSchema> = build_two_alpha_values_graph().into();
    let slot_index =
        TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Values).index();
    raw.property_storage[slot_index].offsets_mut().pop();

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::OffsetsLengthMismatch { .. }));
}

#[test]
fn offsets_not_starting_at_zero_are_rejected() {
    let mut raw: RawGraph<TestSchema> = build_two_alpha_values_graph().into();
    let slot_index =
        TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Values).index();
    raw.property_storage[slot_index].offsets_mut().swap(0, 2);

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::OffsetsBoundsMismatch { .. }));
}

#[test]
fn offsets_end_not_matching_values_length_is_rejected() {
    let mut raw: RawGraph<TestSchema> = build_two_alpha_values_graph().into();
    let slot_index =
        TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Values).index();
    raw.property_storage[slot_index]
        .values_mut()
        .try_as_string_mut()
        .unwrap()
        .pop();

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::OffsetsBoundsMismatch { .. }));
}

#[test]
fn storage_type_mismatch_is_rejected() {
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

    let mut raw: RawGraph<TestSchema> = graph.into();
    let slot_index = TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Key).index();
    *raw.property_storage[slot_index].values_mut() = StorageArray::Int(vec![7]);

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::InvalidPropertyType { .. }));
}

#[test]
fn dangling_node_id_out_of_bounds_is_rejected() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut raw: RawGraph<TestSchema> = graph.into();
    shrink_node_count(&mut raw, TestNode::Beta);

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::NodeSeqOutOfBounds { .. }));
}

#[test]
fn foreign_string_id_is_rejected() {
    // graph_a interns 3 strings ("foo", "extra1", "extra2"); take the id of the *last* one so
    // its index is guaranteed to exceed graph_b's much smaller pool, rather than coincidentally
    // aliasing index 0 in both pools (every fresh graph interns its first string at index 0).
    let mut diff_a = GraphDiff::<TestSchema>::default();
    diff_a.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "foo".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "extra1".to_string())
            .unwrap()
            .add_property(TestProperty::Values, "extra2".to_string())
            .unwrap()
            .build(),
    );
    let (graph_a, _) = diff_a.apply(Graph::new()).expect("apply a");
    graph_a
        .check_integrity()
        .expect("graph passes integrity check");
    let raw_a: RawGraph<TestSchema> = graph_a.into();
    let values_slot_index =
        TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Values).index();
    let foreign_id: RawStringId = raw_a.property_storage[values_slot_index]
        .values()
        .try_as_string()
        .unwrap()[1];

    let mut diff_b = GraphDiff::<TestSchema>::default();
    diff_b.add_node(
        builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, "bar".to_string())
            .unwrap()
            .build(),
    );
    let (graph_b, _) = diff_b.apply(Graph::new()).expect("apply b");
    graph_b
        .check_integrity()
        .expect("graph passes integrity check");
    let mut raw_b: RawGraph<TestSchema> = graph_b.into();
    let key_slot_index =
        TestSchema::property_storage_slot(TestNode::Alpha, TestProperty::Key).index();
    raw_b.property_storage[key_slot_index]
        .values_mut()
        .try_as_string_mut()
        .unwrap()[0] = foreign_id;

    let err = Graph::<TestSchema>::try_from(raw_b)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::UnresolvedStringId(_)));
}

#[test]
fn unpaired_half_edge_is_rejected() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut raw: RawGraph<TestSchema> = graph.into();
    let in_plain_index =
        TestSchema::edge_storage_slot(TestNode::Beta, Direction::In, TestEdge::Plain).index();
    let slot = &mut raw.edge_storage[in_plain_index];

    let offsets = slot.offsets_mut();
    let last = offsets.len() - 1;
    offsets[last] = Offset::zero();
    slot.neighbors_mut().clear();

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
}

/// Regression test for a HashSet-vs-multiset bug caught in design review: a plain existence
/// check would see the mirror key for `(Alpha, Out, Plain, Beta)` still present (since one of
/// Beta's two In-Plain halves survives) and incorrectly call the graph paired. Only a counting
/// check catches the degree mismatch (2 forward halves, 1 reverse half).
#[test]
fn parallel_edge_degree_mismatch_is_rejected() {
    let mut setup = GraphDiff::<TestSchema>::default();
    let alpha_id = setup.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = setup.add_node(builders::BetaNodeBuilder::new().build());
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    setup.add_edge(alpha_id, beta_id, TestEdge::Plain, None);
    setup.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Labeled,
        Some(PropertyValue::String("l0".to_string())),
    );
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let mut raw: RawGraph<TestSchema> = graph.into();
    let alpha_out_labeled_index =
        TestSchema::edge_storage_slot(TestNode::Alpha, Direction::Out, TestEdge::Labeled).index();
    let beta_in_plain_index =
        TestSchema::edge_storage_slot(TestNode::Beta, Direction::In, TestEdge::Plain).index();

    let one = raw.edge_storage[alpha_out_labeled_index].offsets()[1];
    let slot = &mut raw.edge_storage[beta_in_plain_index];
    let offsets = slot.offsets_mut();
    let last = offsets.len() - 1;
    offsets[last] = one;
    slot.neighbors_mut().pop();

    let err = Graph::<TestSchema>::try_from(raw)
        .err()
        .expect("expected an error");
    assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
}
