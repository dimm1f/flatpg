use flatpg::error::Error;
use test_fixtures::*;

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
