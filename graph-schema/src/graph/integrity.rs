//! Integrity checking for a graph's flat CSR storage.
//!
//! [`CheckIntegrity::check_integrity`] checks that the graph's storage is valid. Normally
//! [`crate::graph::builder::GraphDiff::apply`] keeps this true step by step as it builds the
//! graph. This check verifies the same rules directly on the final data: offset arrays are
//! well-formed, storage slot types match the schema, node/string/enum references point to real
//! data, and every edge has a matching reverse edge. `TryFrom<RawGraph<S>> for Graph<S>` runs
//! this check before returning a valid [`Graph<S>`].
//!
//! Known limitations:
//! - Enum id checks only confirm that a [`RawEnumId`] points to *some* registered enum
//!   with a valid variant. They do not confirm it is the *specific* enum that a property or
//!   edge slot's schema expects. That would need a schema API this crate does not have yet: a
//!   way to ask "which registry index does this Enum-typed kind expect?"
//! - Edge pairing checks only confirm that both directions have the same number of edges
//!   (degree symmetry). They do not check that the edges' property values match, since
//!   `StoredProperty` does not implement `PartialEq`/`Eq`.

use std::collections::HashMap;

use crate::{
    EnumPropertyRegistry, ItemAsStr, ItemIndex,
    edge::Direction,
    enum_property::RawEnumId,
    error::Error,
    node::RawNodeId,
    property::PropertyType,
    schema::Schema,
    storage::{EdgeStorage, NodeMetaStorage, Offset, PropertyStorage, StorageArray},
    strings_pool::{RawStringId, StringsPool},
};

/// Verifies a graph's flat storage is well-formed. See the module docs for known limitations.
///
/// Implemented for both `Graph<S>` (in `graph/mod.rs`) and `RawGraph<S>` (in `graph/raw.rs`),
/// next to each type's own definition; both delegate to [`check_integrity`] here.
pub trait CheckIntegrity<S: Schema> {
    fn check_integrity(&self) -> Result<(), Error>;
}

pub(crate) fn check_integrity<S: Schema>(
    node_meta_storage: &NodeMetaStorage<S>,
    edge_storage: &EdgeStorage<S>,
    property_storage: &PropertyStorage<S>,
    strings: &StringsPool,
) -> Result<(), Error> {
    check_storage_sizes::<S>(node_meta_storage, edge_storage, property_storage)?;

    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        let slot = S::property_storage_slot(node_kind, property_kind);
        let slot_name = slot.to_string();
        let expected_count = node_meta_storage[node_kind.index()].len();
        let expected_type = S::node_property_type(property_kind);

        let offsets = property_storage[slot.offset_index()].try_as_offset()?;
        check_offsets_shape(&slot_name, offsets, expected_count)?;

        let values = &property_storage[slot.values_index()];
        check_storage_type(values, expected_type)?;
        if expected_type != PropertyType::None {
            check_offsets_bounds(&slot_name, offsets, values.len())?;
        }

        check_values_content::<S>(values, node_meta_storage, strings)?;
    }

    let mut half_edge_counts: HashMap<(RawNodeId, Direction, usize, RawNodeId), usize> =
        HashMap::new();

    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        let slot = S::edge_storage_slot(node_kind, direction, edge_kind);
        let slot_name = slot.to_string();
        let expected_count = node_meta_storage[node_kind.index()].len();

        let offsets = edge_storage[slot.offset_index()].try_as_offset()?;
        check_offsets_shape(&slot_name, offsets, expected_count)?;

        let neighbors_arr = &edge_storage[slot.neighbors_index()];
        check_storage_type(neighbors_arr, PropertyType::NodeId)?;
        check_offsets_bounds(&slot_name, offsets, neighbors_arr.len())?;
        check_values_content::<S>(neighbors_arr, node_meta_storage, strings)?;
        let neighbors = neighbors_arr.try_as_node_id()?;

        let properties_arr = &edge_storage[slot.properties_index()];
        let expected_prop_type = S::edge_property_type(edge_kind);
        check_storage_type(properties_arr, expected_prop_type)?;
        if expected_prop_type != PropertyType::None {
            check_offsets_bounds(&slot_name, offsets, properties_arr.len())?;
        }
        check_values_content::<S>(properties_arr, node_meta_storage, strings)?;

        for seq in 0..expected_count {
            let start = offsets[seq].value();
            let end = offsets[seq + 1].value();
            let node = RawNodeId::new(node_kind.index(), seq);
            for &neighbor in &neighbors[start..end] {
                *half_edge_counts
                    .entry((node, direction, edge_kind.index(), neighbor))
                    .or_insert(0) += 1;
            }
        }
    }

    check_half_edge_pairing::<S>(&half_edge_counts)?;

    Ok(())
}

fn check_storage_sizes<S: Schema>(
    node_meta_storage: &NodeMetaStorage<S>,
    edge_storage: &EdgeStorage<S>,
    property_storage: &PropertyStorage<S>,
) -> Result<(), Error> {
    if node_meta_storage.len() != S::number_of_node_kinds() {
        return Err(Error::storage_size_mismatch(
            "node_meta_storage",
            S::number_of_node_kinds(),
            node_meta_storage.len(),
        ));
    }
    if edge_storage.len() != S::edge_storage_size() {
        return Err(Error::storage_size_mismatch(
            "edge_storage",
            S::edge_storage_size(),
            edge_storage.len(),
        ));
    }
    if property_storage.len() != S::property_storage_size() {
        return Err(Error::storage_size_mismatch(
            "property_storage",
            S::property_storage_size(),
            property_storage.len(),
        ));
    }
    Ok(())
}

/// Checks an offsets array: its length must be `node_count + 1`, it must start at zero, and
/// each value must be greater than or equal to the one before it. The non-decreasing check
/// uses [`Offset::checked_sub`], since `Offset` guarantees it can never go negative.
fn check_offsets_shape(slot: &str, offsets: &[Offset], expected_count: usize) -> Result<(), Error> {
    if offsets.len() != expected_count + 1 {
        return Err(Error::offsets_length_mismatch(
            slot,
            expected_count + 1,
            offsets.len(),
        ));
    }

    let first = offsets.first().copied().unwrap_or_else(Offset::zero);
    if first.value() != 0 {
        return Err(Error::offsets_bounds_mismatch(slot, 0, first.value()));
    }

    for window in offsets.windows(2) {
        window[1].checked_sub(window[0])?;
    }

    Ok(())
}

/// Checks that the last value in an offsets array equals the length of its paired
/// values/neighbors array. The caller skips this check for edge `properties` slots typed
/// [`PropertyType::None`], because those slots are not resized together with `neighbors`
/// (see module docs).
fn check_offsets_bounds(slot: &str, offsets: &[Offset], array_len: usize) -> Result<(), Error> {
    let last = offsets.last().copied().unwrap_or_else(Offset::zero);
    if last.value() != array_len {
        return Err(Error::offsets_bounds_mismatch(
            slot,
            array_len,
            last.value(),
        ));
    }
    Ok(())
}

/// Checks that a `StorageArray`'s variant matches the type declared in the schema.
///
/// When `expected` is `PropertyType::None`, this checks the variant directly instead of
/// calling `.typ()`. That is because both `StorageArray::Offset` and `StorageArray::None`
/// report `PropertyType::None` from `.typ()`, so `.typ()` alone cannot tell them apart.
fn check_storage_type(storage: &StorageArray, expected: PropertyType) -> Result<(), Error> {
    let matches = match expected {
        PropertyType::None => matches!(storage, StorageArray::None),
        _ => storage.typ() == expected,
    };
    if matches {
        Ok(())
    } else {
        Err(Error::invalid_property_type(expected, storage.typ()))
    }
}

/// Validates every `RawNodeId`/`RawStringId`/`RawEnumId` embedded in a storage array, whatever its
/// variant turns out to be. A no-op for scalar-typed or empty/`None` arrays.
fn check_values_content<S: Schema>(
    storage: &StorageArray,
    node_meta_storage: &NodeMetaStorage<S>,
    strings: &StringsPool,
) -> Result<(), Error> {
    match storage {
        StorageArray::NodeId(items) => {
            for &node in items {
                check_node_id::<S>(node, node_meta_storage)?;
            }
        }
        StorageArray::StringId(items) => {
            for &string_id in items {
                check_string_id(string_id, strings)?;
            }
        }
        StorageArray::Enum(items) => {
            for &enum_id in items {
                check_enum_id::<S::EPR>(enum_id)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn check_node_id<S: Schema>(
    node: RawNodeId,
    node_meta_storage: &NodeMetaStorage<S>,
) -> Result<(), Error> {
    let kind = S::resolve_node_kind(node)?;
    let count = node_meta_storage[kind.index()].len();
    if node.seq() >= count {
        return Err(Error::node_seq_out_of_bounds(
            node.to_string(),
            node.seq(),
            count,
        ));
    }
    Ok(())
}

fn check_string_id(string_id: RawStringId, strings: &StringsPool) -> Result<(), Error> {
    if strings.get(string_id).is_none() {
        return Err(Error::unresolved_string_id(string_id.to_string()));
    }
    Ok(())
}

fn check_enum_id<EPR: EnumPropertyRegistry>(enum_id: RawEnumId) -> Result<(), Error> {
    let registry_kind = EPR::from_index(enum_id.enum_property_index())
        .ok_or_else(|| Error::unresolved_enum_kind(enum_id.enum_property_index()))?;
    if enum_id.variant() >= registry_kind.variant_count() {
        return Err(Error::unresolved_enum_variant(
            registry_kind.as_str(),
            enum_id.variant(),
        ));
    }
    Ok(())
}

/// Confirms every half-edge has a matching reverse half, with the same count.
///
/// This counts how many times each half-edge appears, instead of just checking that a
/// reverse half-edge exists. That way it can catch a mismatched count of parallel edges.
/// Example: two `Out` edges from A to B, but only one `In` edge back from B to A. A simple
/// existence check would miss this, since a matching reverse edge does exist — just not
/// enough of them.
fn check_half_edge_pairing<S: Schema>(
    half_edge_counts: &HashMap<(RawNodeId, Direction, usize, RawNodeId), usize>,
) -> Result<(), Error> {
    for (&(node, direction, edge_kind_index, neighbor), &count) in half_edge_counts {
        let opposite = match direction {
            Direction::In => Direction::Out,
            Direction::Out => Direction::In,
        };
        let mirror_count = half_edge_counts
            .get(&(neighbor, opposite, edge_kind_index, node))
            .copied()
            .unwrap_or(0);

        if mirror_count != count {
            let edge_kind_name = S::edge_kind_by_index(edge_kind_index)
                .map(|k| k.as_str().to_string())
                .unwrap_or_default();
            return Err(Error::reverse_edge_not_found(
                neighbor.to_string(),
                node.to_string(),
                direction.as_str().to_string(),
                edge_kind_name,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::{ItemAll, ItemFromIndex, ItemFromStr};

    #[test]
    fn offsets_shape_accepts_valid() {
        let offsets = [
            Offset::new(0).unwrap(),
            Offset::new(2).unwrap(),
            Offset::new(5).unwrap(),
        ];
        assert!(check_offsets_shape("slot", &offsets, 2).is_ok());
    }

    #[test]
    fn offsets_shape_rejects_non_monotonic() {
        let offsets = [
            Offset::new(0).unwrap(),
            Offset::new(5).unwrap(),
            Offset::new(2).unwrap(),
        ];
        let err = check_offsets_shape("slot", &offsets, 2).unwrap_err();
        assert!(matches!(err, Error::OffsetUnderflow));
    }

    #[test]
    fn offsets_shape_rejects_wrong_length() {
        let offsets = [Offset::new(0).unwrap(), Offset::new(3).unwrap()];
        let err = check_offsets_shape("slot", &offsets, 5).unwrap_err();
        assert!(matches!(err, Error::OffsetsLengthMismatch { .. }));
    }

    #[test]
    fn offsets_shape_rejects_nonzero_start() {
        let offsets = [Offset::new(1).unwrap(), Offset::new(3).unwrap()];
        let err = check_offsets_shape("slot", &offsets, 1).unwrap_err();
        assert!(matches!(err, Error::OffsetsBoundsMismatch { .. }));
    }

    #[test]
    fn offsets_bounds_accepts_matching_end() {
        let offsets = [Offset::new(0).unwrap(), Offset::new(5).unwrap()];
        assert!(check_offsets_bounds("slot", &offsets, 5).is_ok());
    }

    #[test]
    fn offsets_bounds_rejects_mismatched_end() {
        let offsets = [Offset::new(0).unwrap(), Offset::new(5).unwrap()];
        let err = check_offsets_bounds("slot", &offsets, 3).unwrap_err();
        assert!(matches!(err, Error::OffsetsBoundsMismatch { .. }));
    }

    #[test]
    fn storage_type_accepts_match() {
        assert!(check_storage_type(&StorageArray::Int(vec![7]), PropertyType::Int).is_ok());
    }

    #[test]
    fn storage_type_rejects_mismatch() {
        let err =
            check_storage_type(&StorageArray::Int(vec![7]), PropertyType::String).unwrap_err();
        assert!(matches!(err, Error::InvalidPropertyType { .. }));
    }

    #[test]
    fn storage_type_distinguishes_offset_from_none() {
        // Regression test for the case described in check_storage_type's doc comment: an Offset
        // array and a real, empty None-typed slot both report PropertyType::None from `.typ()`.
        // This test makes sure they are NOT treated as a match.
        let err =
            check_storage_type(&StorageArray::Offset(vec![]), PropertyType::None).unwrap_err();
        assert!(matches!(err, Error::InvalidPropertyType { .. }));
    }

    #[test]
    fn string_id_accepts_valid_id() {
        let mut pool = StringsPool::new();
        let valid = pool.intern("foo");
        assert!(check_string_id(valid, &pool).is_ok());
    }

    #[test]
    fn string_id_rejects_foreign_id() {
        let mut pool_a = StringsPool::new();
        let foreign = pool_a.intern("foo");
        let pool_b = StringsPool::new();
        let err = check_string_id(foreign, &pool_b).unwrap_err();
        assert!(matches!(err, Error::UnresolvedStringId(_)));
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestRegistry {
        Status,
    }

    impl ItemAsStr for TestRegistry {
        fn as_str(&self) -> &'static str {
            "Status"
        }
    }
    impl ItemIndex for TestRegistry {
        fn index(&self) -> usize {
            0
        }
    }
    impl ItemFromIndex for TestRegistry {
        fn from_index(index: usize) -> Option<Self> {
            (index == 0).then_some(Self::Status)
        }
    }
    impl ItemAll for TestRegistry {
        fn all() -> &'static [Self] {
            &[Self::Status]
        }
    }
    impl FromStr for TestRegistry {
        type Err = Error;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            if s == "Status" {
                Ok(Self::Status)
            } else {
                Err(Error::unknown_label("TestRegistry", s))
            }
        }
    }
    impl ItemFromStr for TestRegistry {}
    impl EnumPropertyRegistry for TestRegistry {
        fn variant_count(&self) -> usize {
            match self {
                // Hardcoded: check_enum_id only needs EnumPropertyRegistry, not a full Schema,
                // so there's no second "domain enum" type to delegate to here (unlike what
                // #[derive(EnumPropertyRegistry)] generates in production).
                Self::Status => 2,
            }
        }
    }

    #[test]
    fn enum_id_within_variant_range_is_accepted() {
        let valid = RawEnumId::new(0, 1);
        assert!(check_enum_id::<TestRegistry>(valid).is_ok());
    }

    #[test]
    fn enum_id_variant_out_of_range_is_rejected() {
        let out_of_range = RawEnumId::new(0, 2);
        let err = check_enum_id::<TestRegistry>(out_of_range).unwrap_err();
        assert!(matches!(err, Error::UnresolvedEnumVariant { .. }));
    }

    #[test]
    fn enum_id_unregistered_index_is_rejected() {
        let unregistered = RawEnumId::new(1, 0);
        let err = check_enum_id::<TestRegistry>(unregistered).unwrap_err();
        assert!(matches!(err, Error::UnresolvedEnumKind(_)));
    }
}
