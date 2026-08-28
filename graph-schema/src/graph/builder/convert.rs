use crate::{
    error::Error,
    property::{PropertyType, PropertyValue},
    storage::{StorageArray, StoredProperty},
    strings_pool::StringsPool,
};

pub(super) fn stored_property_batch(
    values: &[PropertyValue],
    typ: PropertyType,
    strings: &mut StringsPool,
) -> Result<StorageArray, Error> {
    let mut batch = StorageArray::with_capacity(typ, values.len());
    for prop in values {
        let prop = to_stored_property(prop, strings);
        batch.try_push(&prop)?;
    }
    Ok(batch)
}

pub(super) fn to_stored_property(
    prop: &PropertyValue,
    strings: &mut StringsPool,
) -> StoredProperty {
    match prop {
        PropertyValue::Bool(v) => StoredProperty::Bool(*v),
        PropertyValue::Byte(v) => StoredProperty::Byte(*v),
        PropertyValue::Short(v) => StoredProperty::Short(*v),
        PropertyValue::Int(v) => StoredProperty::Int(*v),
        PropertyValue::Long(v) => StoredProperty::Long(*v),
        PropertyValue::Float(v) => StoredProperty::Float(*v),
        PropertyValue::Double(v) => StoredProperty::Double(*v),
        PropertyValue::NodeId(node_ref) => StoredProperty::NodeId(*node_ref),
        PropertyValue::String(s) => StoredProperty::StringId(strings.intern(s)),
        PropertyValue::Enum(v) => StoredProperty::Enum(*v),
    }
}
