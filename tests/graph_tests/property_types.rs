use flatpg::{
    edge::Direction,
    graph::{Graph, builder::GraphDiff},
    node::RawNodeId,
    prelude::*,
    property::PropertyValue,
};
use test_fixtures::*;

use crate::common::collect_edges;

#[test]
fn gamma_node_scalar_properties_round_trip() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::GammaNodeBuilder::new()
            .add_property(TestProperty::Flag, true)
            .unwrap()
            .add_property(TestProperty::Level, 7u8)
            .unwrap()
            .add_property(TestProperty::Rank, 42i16)
            .unwrap()
            .add_property(TestProperty::BigCount, 1_000_000_000_000i64)
            .unwrap()
            .add_property(TestProperty::Ratio, 1.5f32)
            .unwrap()
            .add_property(TestProperty::Score, 2.5f64)
            .unwrap()
            .build(),
    );

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let gamma = GammaNode::new(&graph, 0);

    assert!(gamma.flag().unwrap());
    assert_eq!(gamma.level().unwrap(), 7);
    assert_eq!(gamma.rank().unwrap(), 42);
    assert_eq!(gamma.big_count().unwrap(), 1_000_000_000_000);
    assert_eq!(gamma.ratio().unwrap(), 1.5);
    assert_eq!(gamma.score().unwrap(), 2.5);
}

#[test]
#[allow(deprecated)]
fn gamma_node_with_node_id_property_round_trips() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::GammaNodeBuilder::new()
            .add_property(TestProperty::LinkedNode, alpha)
            .unwrap()
            .build(),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let gamma = GammaNode::new(&graph, 0);
    let linked = gamma.linked_node().unwrap();
    assert_eq!(linked.kind(), TestNode::Alpha);
    assert_eq!(linked.seq(), alpha.seq());
}

#[test]
fn gamma_node_multi_valued_enum_property_round_trips() {
    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_node(
        builders::GammaNodeBuilder::new()
            .add_property(TestProperty::Tags, Status::Active)
            .unwrap()
            .add_property(TestProperty::Tags, Status::Banned)
            .unwrap()
            .build(),
    );

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let gamma = GammaNode::new(&graph, 0);

    assert_eq!(gamma.tags().unwrap(), vec![Status::Active, Status::Banned]);
}

#[test]
fn edge_scalar_properties_round_trip() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Active,
        Some(PropertyValue::Bool(true)),
    );
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Weight,
        Some(PropertyValue::Byte(9)),
    );
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Priority,
        Some(PropertyValue::Short(-3)),
    );
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Distance,
        Some(PropertyValue::Int(120)),
    );
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Timestamp,
        Some(PropertyValue::Long(9_999_999_999)),
    );
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Fraction,
        Some(PropertyValue::Float(0.25)),
    );
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Precision,
        Some(PropertyValue::Double(0.125)),
    );

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let edge_of = |kind: TestEdge| {
        collect_edges(&graph, alpha, kind, Direction::Out)
            .into_iter()
            .next()
            .expect("one edge")
    };

    let active_id = edge_of(TestEdge::Active);
    let active = ActiveEdge::new(
        &graph,
        active_id.src_node(),
        active_id.dst_node(),
        active_id.direction(),
        active_id.seq(),
    );
    assert!(active.property().unwrap().unwrap());

    let weight_id = edge_of(TestEdge::Weight);
    let weight = WeightEdge::new(
        &graph,
        weight_id.src_node(),
        weight_id.dst_node(),
        weight_id.direction(),
        weight_id.seq(),
    );
    assert_eq!(weight.property().unwrap().unwrap(), 9);

    let priority_id = edge_of(TestEdge::Priority);
    let priority = PriorityEdge::new(
        &graph,
        priority_id.src_node(),
        priority_id.dst_node(),
        priority_id.direction(),
        priority_id.seq(),
    );
    assert_eq!(priority.property().unwrap().unwrap(), -3);

    let distance_id = edge_of(TestEdge::Distance);
    let distance = DistanceEdge::new(
        &graph,
        distance_id.src_node(),
        distance_id.dst_node(),
        distance_id.direction(),
        distance_id.seq(),
    );
    assert_eq!(distance.property().unwrap().unwrap(), 120);

    let timestamp_id = edge_of(TestEdge::Timestamp);
    let timestamp = TimestampEdge::new(
        &graph,
        timestamp_id.src_node(),
        timestamp_id.dst_node(),
        timestamp_id.direction(),
        timestamp_id.seq(),
    );
    assert_eq!(timestamp.property().unwrap().unwrap(), 9_999_999_999);

    let fraction_id = edge_of(TestEdge::Fraction);
    let fraction = FractionEdge::new(
        &graph,
        fraction_id.src_node(),
        fraction_id.dst_node(),
        fraction_id.direction(),
        fraction_id.seq(),
    );
    assert_eq!(fraction.property().unwrap().unwrap(), 0.25);

    let precision_id = edge_of(TestEdge::Precision);
    let precision = PrecisionEdge::new(
        &graph,
        precision_id.src_node(),
        precision_id.dst_node(),
        precision_id.direction(),
        precision_id.seq(),
    );
    assert_eq!(precision.property().unwrap().unwrap(), 0.125);
}

#[test]
#[allow(deprecated)]
fn edge_with_node_id_property_round_trips() {
    let mut setup = GraphDiff::<TestSchema>::default();
    setup.add_node(builders::AlphaNodeBuilder::new().build());
    setup.add_node(builders::BetaNodeBuilder::new().build());
    let (graph, _) = setup.apply(Graph::new()).expect("apply setup");
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

    let mut diff = GraphDiff::<TestSchema>::default();
    diff.add_edge(
        RawNodeId::from(&alpha),
        RawNodeId::from(&beta),
        TestEdge::RefersTo,
        Some(PropertyValue::from(beta)),
    );
    let (graph, _) = diff.apply(graph).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");

    let edge_id = collect_edges(&graph, alpha, TestEdge::RefersTo, Direction::Out)
        .into_iter()
        .next()
        .expect("one edge");
    let refers_to = RefersToEdge::new(
        &graph,
        edge_id.src_node(),
        edge_id.dst_node(),
        edge_id.direction(),
        edge_id.seq(),
    );

    let target = refers_to.property().unwrap().unwrap();
    assert_eq!(target.kind(), TestNode::Beta);
    assert_eq!(target.seq(), beta.seq());
}

#[test]
fn property_rename_overrides_string_representation() {
    assert_eq!(TestProperty::Tag.as_str(), "Label");
    assert_eq!("Label".parse::<TestProperty>().unwrap(), TestProperty::Tag);
    assert!("Tag".parse::<TestProperty>().is_err());
}

#[test]
fn edge_with_enum_property_round_trips() {
    let mut diff = GraphDiff::<TestSchema>::default();
    let alpha_id = diff.add_node(builders::AlphaNodeBuilder::new().build());
    let beta_id = diff.add_node(builders::BetaNodeBuilder::new().build());
    diff.add_edge(
        alpha_id,
        beta_id,
        TestEdge::Tagged,
        Some(PropertyValue::from(Status::Inactive)),
    );

    let (graph, _) = diff.apply(Graph::new()).expect("apply diff");
    graph
        .check_integrity()
        .expect("graph passes integrity check");
    let alpha = graph
        .nodes_by_kind(TestNode::Alpha)
        .next()
        .expect("Alpha node");

    let edge_id = collect_edges(&graph, alpha, TestEdge::Tagged, Direction::Out)
        .into_iter()
        .next()
        .expect("one edge");
    let tagged = TaggedEdge::new(
        &graph,
        edge_id.src_node(),
        edge_id.dst_node(),
        edge_id.direction(),
        edge_id.seq(),
    );
    assert_eq!(tagged.property().unwrap().unwrap(), Status::Inactive);
}
