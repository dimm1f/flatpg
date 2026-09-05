//! Integrity checking for a graph's flat CSR storage.
//!
//! [`CheckIntegrity::check_integrity`] checks that the graph's storage is valid. Normally
//! [`crate::graph::builder::GraphDiff::apply`] keeps this true step by step as it builds the
//! graph. This check verifies the same rules directly on the final data: offset arrays are
//! well-formed, storage slot types match the schema, node/string/enum references point to real
//! data, and every edge has a matching reverse edge. `TryFrom<RawGraph<S>> for Graph<S>` runs
//! this check before returning a valid [`Graph<S>`](crate::graph::Graph).
//!
//! Known limitations:
//! - Enum id checks only confirm that a [`RawEnumId`] points to *some* registered enum
//!   with a valid variant. They do not confirm it is the *specific* enum that a property or
//!   edge slot's schema expects. That would need a schema API this crate does not have yet: a
//!   way to ask "which registry index does this Enum-typed kind expect?"
//! - Edge pairing checks only confirm that both directions have the same number of edges
//!   (degree symmetry). They do not check that the edges' property values match, since
//!   `StoredProperty` does not implement `PartialEq`/`Eq`.

use std::{cmp::Ordering, fmt::Display};

use crate::{
    EnumPropertyRegistry, ItemAsStr, ItemIndex,
    edge::Direction,
    enum_property::RawEnumId,
    error::Error,
    node::RawNodeId,
    property::PropertyType,
    schema::Schema,
    storage::{EdgeStorage, NodeMetaStorage, Offset, OffsetStorage, PropertyStorage, StorageArray},
    strings_pool::{RawStringId, StringsPool},
};

/// A half-edge canonicalized as a `(source, destination)` pair, so that the two halves of
/// one edge produce the same value whichever endpoint they are stored on.
///
/// The edge kind is not part of the value: halves are bucketed by `edge_kind.index()` and
/// compared only within one bucket, so edges of different kinds between the same pair of nodes
/// never pair up with each other. Node kind needs no bucketing, since [`RawNodeId`] already
/// carries it alongside the sequence number.
type HalfEdge = (RawNodeId, RawNodeId);

struct Storages<'a, S: Schema> {
    node_meta: &'a NodeMetaStorage<S>,
    edges: &'a EdgeStorage<S>,
    properties: &'a PropertyStorage<S>,
    strings: &'a StringsPool,
}

/// Verifies a graph's flat storage is well-formed. See the module docs for known limitations.
///
/// Implemented for both `Graph<S>` (in `graph/mod.rs`) and `RawGraph<S>` (in `graph/raw.rs`),
/// next to each type's own definition; both delegate to the private `check_integrity` free
/// function in this module.
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

    let storages = Storages {
        node_meta: node_meta_storage,
        edges: edge_storage,
        properties: property_storage,
        strings,
    };

    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        check_property_slot(&storages, node_kind, property_kind)?;
    }

    let mut out_halves = vec![Vec::new(); S::number_of_edge_kinds()];
    let mut in_halves = vec![Vec::new(); S::number_of_edge_kinds()];

    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        let halves = match direction {
            Direction::Out => &mut out_halves[edge_kind.index()],
            Direction::In => &mut in_halves[edge_kind.index()],
        };
        check_edge_slot(&storages, node_kind, direction, edge_kind, halves)?;
    }

    for &edge_kind in S::edge_kinds() {
        check_half_edge_pairing(
            edge_kind,
            &mut out_halves[edge_kind.index()],
            &mut in_halves[edge_kind.index()],
        )?;
    }

    Ok(())
}

/// Checks one node property storage slot, and the node/string/enum ids its values embed.
fn check_property_slot<S: Schema>(
    storages: &Storages<'_, S>,
    node_kind: S::N,
    property_kind: S::P,
) -> Result<(), Error> {
    let slot_index = S::property_storage_slot(node_kind, property_kind);
    let expected_count = storages.node_meta[node_kind.index()].len();
    let expected_type = S::node_property_type(property_kind);

    let slot = &storages.properties[slot_index.index()];
    check_offsets_shape(slot_index, slot.offsets(), expected_count)?;
    check_storage_type(slot.values(), expected_type)?;
    if expected_type != PropertyType::None {
        check_offsets_bounds(slot_index, slot.offsets(), slot.values().len())?;
    }

    check_values_content::<S>(slot.values(), storages.node_meta, storages.strings)
}

/// Checks one edge storage slot, appending its half-edges to `halves` in canonical
/// `(source, destination)` form for [`check_half_edge_pairing`].
fn check_edge_slot<S: Schema>(
    storages: &Storages<'_, S>,
    node_kind: S::N,
    direction: Direction,
    edge_kind: S::E,
    halves: &mut Vec<HalfEdge>,
) -> Result<(), Error> {
    let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind);
    let expected_count = storages.node_meta[node_kind.index()].len();

    let slot = &storages.edges[slot_index.index()];
    check_offsets_shape(slot_index, slot.offsets(), expected_count)?;

    check_offsets_bounds(slot_index, slot.offsets(), slot.neighbors().len())?;
    for node_id in slot.neighbors() {
        check_node_id::<S>(*node_id, storages.node_meta)?;
    }

    let expected_prop_type = S::edge_property_type(edge_kind);
    check_storage_type(slot.values(), expected_prop_type)?;
    if expected_prop_type != PropertyType::None {
        check_offsets_bounds(slot_index, slot.offsets(), slot.values().len())?;
    }
    check_values_content::<S>(slot.values(), storages.node_meta, storages.strings)?;

    halves.reserve(slot.neighbors().len());
    for (seq, window) in slot.offsets().windows(2).enumerate() {
        let node = RawNodeId::new(node_kind.index(), seq);
        for neighbor in slot.get_neighbors(window[0], window[1]) {
            halves.push(match direction {
                Direction::Out => (node, neighbor),
                Direction::In => (neighbor, node),
            });
        }
    }

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
fn check_offsets_shape(
    slot: impl Display,
    offsets: &[Offset],
    expected_count: usize,
) -> Result<(), Error> {
    if offsets.is_empty() {
        return Ok(());
    }
    if offsets.len() != expected_count + 1 {
        return Err(Error::offsets_length_mismatch(
            slot.to_string(),
            expected_count + 1,
            offsets.len(),
        ));
    }

    let first = offsets.first().copied().unwrap_or_else(Offset::zero);
    if first.value() != 0 {
        return Err(Error::offsets_bounds_mismatch(
            slot.to_string(),
            0,
            first.value(),
        ));
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
fn check_offsets_bounds(
    slot: impl Display,
    offsets: &[Offset],
    array_len: usize,
) -> Result<(), Error> {
    let last = offsets.last().copied().unwrap_or_else(Offset::zero);
    if last.value() != array_len {
        return Err(Error::offsets_bounds_mismatch(
            slot.to_string(),
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

/// Confirms every half-edge of one edge kind has a matching reverse half, with the same count.
///
/// Both halves of an edge canonicalize to the same `(source, destination)` pair, so the two
/// sides pair up exactly when `out_halves` and `in_halves` hold equal multisets. Sorting
/// makes that comparison a linear scan and pins the reported error to the lowest-ordered
/// unpaired half. Both slices are left sorted.
///
/// Comparing multisets rather than testing each half for the mere existence of a reverse
/// catches a mismatched count of parallel edges: two `Out` edges from A to B against a
/// single `In` edge back from B to A does have a matching reverse edge — just not enough
/// of them.
fn check_half_edge_pairing<E: ItemAsStr>(
    edge_kind: E,
    out_halves: &mut [HalfEdge],
    in_halves: &mut [HalfEdge],
) -> Result<(), Error> {
    out_halves.sort_unstable();
    in_halves.sort_unstable();

    let mismatch = out_halves
        .iter()
        .zip(in_halves.iter())
        .position(|(out_half, in_half)| out_half != in_half);

    let ((src, dst), direction) = match mismatch {
        Some(i) if out_halves[i] < in_halves[i] => (out_halves[i], Direction::Out),
        Some(i) => (in_halves[i], Direction::In),
        None => match out_halves.len().cmp(&in_halves.len()) {
            Ordering::Greater => (out_halves[in_halves.len()], Direction::Out),
            Ordering::Less => (in_halves[out_halves.len()], Direction::In),
            Ordering::Equal => return Ok(()),
        },
    };

    let (node, target) = match direction {
        Direction::Out => (src, dst),
        Direction::In => (dst, src),
    };
    Err(Error::reverse_edge_not_found(
        target.to_string(),
        node.to_string(),
        direction.as_str(),
        edge_kind.as_str(),
    ))
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

    fn node(seq: usize) -> RawNodeId {
        RawNodeId::new(0, seq)
    }

    /// Locks in that pairing compares multisets, not slice order: the two sides are built by
    /// walking different storage slots and only agree once sorted.
    #[test]
    fn half_edge_pairing_accepts_reordered_matching_halves() {
        let mut out_halves = [(node(0), node(1)), (node(2), node(3))];
        let mut in_halves = [(node(2), node(3)), (node(0), node(1))];
        assert!(
            check_half_edge_pairing(TestRegistry::Status, &mut out_halves, &mut in_halves).is_ok()
        );
    }

    #[test]
    fn half_edge_pairing_rejects_out_half_without_reverse() {
        let mut out_halves = [(node(0), node(1)), (node(0), node(2))];
        let mut in_halves = [(node(0), node(1))];
        let err = check_half_edge_pairing(TestRegistry::Status, &mut out_halves, &mut in_halves)
            .expect_err("expected an error");

        let Error::ReverseEdgeNotFound {
            target,
            node: owner,
            direction,
            ..
        } = err
        else {
            panic!("expected ReverseEdgeNotFound, got {err:?}");
        };
        assert_eq!(target, node(2).to_string());
        assert_eq!(owner, node(0).to_string());
        assert_eq!(direction, "Out");
    }

    #[test]
    fn half_edge_pairing_rejects_in_half_without_reverse() {
        let mut out_halves = [(node(0), node(1))];
        let mut in_halves = [(node(0), node(1)), (node(0), node(2))];
        let err = check_half_edge_pairing(TestRegistry::Status, &mut out_halves, &mut in_halves)
            .expect_err("expected an error");

        let Error::ReverseEdgeNotFound {
            target,
            node: owner,
            direction,
            ..
        } = err
        else {
            panic!("expected ReverseEdgeNotFound, got {err:?}");
        };
        assert_eq!(target, node(0).to_string());
        assert_eq!(owner, node(2).to_string());
        assert_eq!(direction, "In");
    }

    /// A plain existence check would accept this: the reverse edge does exist, there is just
    /// one of it against two forward halves.
    #[test]
    fn half_edge_pairing_rejects_parallel_edge_count_mismatch() {
        let mut out_halves = [(node(0), node(1)), (node(0), node(1))];
        let mut in_halves = [(node(0), node(1))];
        let err = check_half_edge_pairing(TestRegistry::Status, &mut out_halves, &mut in_halves)
            .expect_err("expected an error");
        assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
    }
}
