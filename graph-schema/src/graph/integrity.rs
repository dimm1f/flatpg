//! Integrity checking for a graph's flat CSR storage.
//!
//! [`CheckIntegrity::check_integrity`] checks that the graph's storage is valid. Normally
//! [`crate::graph::builder::GraphDiff::apply`] keeps this true step by step as it builds the
//! graph. This check verifies the same rules directly on the final data: offset arrays are
//! well-formed, storage slot types match the schema, node/string/enum references point to real
//! data, and every edge has a matching reverse edge carrying the same property value.
//! `TryFrom<RawGraph<S>> for Graph<S>` runs this check before returning a valid
//! [`Graph<S>`](crate::graph::Graph).
//!
//! How values compare when pairing half-edges:
//! - Two halves pair only when they agree on their property value as well as their endpoints.
//!   Values compare through a canonical encoding, so floats compare **bitwise**: `-0.0` and
//!   `0.0` are different values, and a NaN pairs only with an identical bit pattern.
//! - Strings compare by interned id, which is exact because a graph has exactly one
//!   [`StringsPool`] and interning deduplicates.
//! - Edge kinds typed [`PropertyType::None`] carry no per-edge value, so for them pairing is
//!   endpoint-and-degree symmetry as before.
//!
//! Known limitation: string ids are only bounds-checked, because [`StringsPool::get`] cannot
//! tell a foreign handle from its own. A [`RawStringId`] from another pool that happens to be
//! in range is accepted, and resolves to whatever text sits at that index.

use std::{cmp::Ordering, fmt::Display};

use crate::{
    EnumPropertyRegistry, ItemAsStr, ItemIndex,
    edge::Direction,
    enum_property::RawEnumId,
    error::Error,
    node::RawNodeId,
    property::PropertyType,
    schema::Schema,
    storage::{
        EdgeStorage, NodeMetaStorage, Offset, OffsetStorage, PropertyStorage, StorageArray,
        ValueKey,
    },
    strings_pool::{RawStringId, StringsPool},
};

/// A node's position in a single sequence covering every node of every kind, as a `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct DenseNodeId(u32);

/// Maps between [`RawNodeId`] and [`DenseNodeId`] with one prefix-sum table per graph, and
/// names the node kind a densified id belongs to.
#[derive(Debug)]
struct NodeIndex {
    kind_offsets: Vec<u32>,
    kind_labels: Vec<&'static str>,
}

impl NodeIndex {
    fn new(kinds: impl ExactSizeIterator<Item = (&'static str, usize)>) -> Result<Self, Error> {
        let mut kind_offsets = Vec::with_capacity(kinds.len() + 1);
        let mut kind_labels = Vec::with_capacity(kinds.len());
        let mut total: usize = 0;
        for (label, count) in kinds {
            kind_offsets.push(u32::try_from(total).map_err(|_| Error::node_count_overflow(total))?);
            kind_labels.push(label);
            total = total.saturating_add(count);
        }
        kind_offsets.push(u32::try_from(total).map_err(|_| Error::node_count_overflow(total))?);
        Ok(Self {
            kind_offsets,
            kind_labels,
        })
    }

    fn label(&self, dense: DenseNodeId) -> String {
        let node = self.resolve(dense);
        match self.kind_labels.get(node.kind()) {
            Some(kind) => format!("{kind}({})", node.seq()),
            None => node.to_string(),
        }
    }

    fn densify(&self, node: RawNodeId) -> DenseNodeId {
        let base = self
            .kind_offsets
            .get(node.kind())
            .copied()
            .unwrap_or(u32::MAX);
        DenseNodeId(base.saturating_add(node.seq().try_into().unwrap_or(u32::MAX)))
    }

    fn resolve(&self, dense: DenseNodeId) -> RawNodeId {
        let kind = self
            .kind_offsets
            .iter()
            .rposition(|&start| start <= dense.0)
            .unwrap_or(0)
            .min(self.kind_offsets.len().saturating_sub(2));
        RawNodeId::new(kind, (dense.0 - self.kind_offsets[kind]) as usize)
    }
}

/// A half-edge canonicalized as a `(source, destination)` pair plus its property value, so that
/// the two halves of one edge produce the same value whichever endpoint they are stored on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct HalfEdge {
    src: DenseNodeId,
    dst: DenseNodeId,
    value: ValueKey,
}

struct Storages<'a, S: Schema> {
    node_meta: &'a NodeMetaStorage<S>,
    edges: &'a EdgeStorage<S>,
    properties: &'a PropertyStorage<S>,
    strings: &'a StringsPool,
    nodes: NodeIndex,
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
        nodes: NodeIndex::new(
            S::node_kinds()
                .iter()
                .map(|kind| (kind.as_str(), node_meta_storage[kind.index()].len())),
        )?,
    };

    #[cfg(feature = "parallel")]
    if parallel::is_worthwhile(&storages) {
        return parallel::check_slots(&storages);
    }

    check_slots(&storages)
}

/// Checks every storage slot on one thread, then pairs up each edge kind's half-edges.
fn check_slots<S: Schema>(storages: &Storages<'_, S>) -> Result<(), Error> {
    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        check_property_slot(storages, node_kind, property_kind)?;
    }

    let mut out_halves = vec![Vec::new(); S::number_of_edge_kinds()];
    let mut in_halves = vec![Vec::new(); S::number_of_edge_kinds()];

    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        let halves = match direction {
            Direction::Out => &mut out_halves[edge_kind.index()],
            Direction::In => &mut in_halves[edge_kind.index()],
        };
        check_edge_slot(storages, node_kind, direction, edge_kind, halves)?;
    }

    for &edge_kind in S::edge_kinds() {
        check_half_edge_pairing(
            PairingReport {
                edge_kind,
                nodes: &storages.nodes,
            },
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

    check_values_content::<S>(
        slot.values(),
        storages.node_meta,
        storages.strings,
        S::node_property_enum_index(property_kind),
    )
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
    check_values_content::<S>(
        slot.values(),
        storages.node_meta,
        storages.strings,
        S::edge_property_enum_index(edge_kind),
    )?;

    debug_assert_eq!(slot.values().typ(), expected_prop_type);

    let values = slot.values();
    halves.reserve(slot.neighbors().len());
    for (seq, window) in slot.offsets().windows(2).enumerate() {
        let node = RawNodeId::new(node_kind.index(), seq);
        let start = window[0].value();
        let node = storages.nodes.densify(node);
        for (offset, neighbor) in slot.get_neighbors(window[0], window[1]).enumerate() {
            let value = ValueKey::of(values, start + offset);
            let neighbor = storages.nodes.densify(neighbor);
            let (src, dst) = match direction {
                Direction::Out => (node, neighbor),
                Direction::In => (neighbor, node),
            };
            halves.push(HalfEdge { src, dst, value });
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
/// values/neighbors array. The caller skips this check for slots typed
/// [`PropertyType::None`], whose [`StorageArray::None`] is a single marker rather than an
/// array resized alongside `neighbors`.
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
    expected_enum: Option<usize>,
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
                check_enum_id::<S::EPR>(enum_id, expected_enum)?;
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

/// Checks one enum value: that its enum resolves in the registry, that it is the enum the slot
/// declares, and that its variant is in range for that enum.
fn check_enum_id<EPR: EnumPropertyRegistry>(
    enum_id: RawEnumId,
    expected: Option<usize>,
) -> Result<(), Error> {
    let registry_kind = EPR::from_index(enum_id.enum_property_index())
        .ok_or_else(|| Error::unresolved_enum_kind(enum_id.enum_property_index()))?;
    if let Some(expected) = expected {
        if expected != enum_id.enum_property_index() {
            let expected_kind =
                EPR::from_index(expected).ok_or_else(|| Error::unresolved_enum_kind(expected))?;
            return Err(Error::enum_property_index_mismatch(
                expected_kind.as_str(),
                enum_id.enum_property_index(),
            ));
        }
    }
    if enum_id.variant() >= registry_kind.variant_count() {
        return Err(Error::unresolved_enum_variant(
            registry_kind.as_str(),
            enum_id.variant(),
        ));
    }
    Ok(())
}

/// Confirms every half-edge of one edge kind has a matching reverse half, with the same count
/// and the same property value.
///
/// Both halves of an edge canonicalize to the same `(source, destination, value)`, so the two
/// sides pair up exactly when `out_halves` and `in_halves` hold equal multisets. Sorting
/// makes that comparison a linear scan and pins the reported error to the lowest-ordered
/// unpaired half. Both slices are left sorted.
///
/// Comparing multisets rather than testing each half for the mere existence of a reverse
/// catches a mismatched count of parallel edges: two `Out` edges from A to B against a
/// single `In` edge back from B to A does have a matching reverse edge — just not enough
/// of them.
fn check_half_edge_pairing<E: ItemAsStr>(
    report: PairingReport<'_, E>,
    out_halves: &mut [HalfEdge],
    in_halves: &mut [HalfEdge],
) -> Result<(), Error> {
    out_halves.sort_unstable();
    in_halves.sort_unstable();
    report_pairing_mismatch(report, out_halves, in_halves)
}

struct PairingReport<'a, E> {
    edge_kind: E,
    nodes: &'a NodeIndex,
}

/// Reports the lowest-ordered half-edge that has no matching reverse, given both sides already
/// sorted.
fn report_pairing_mismatch<E: ItemAsStr>(
    report: PairingReport<'_, E>,
    out_halves: &[HalfEdge],
    in_halves: &[HalfEdge],
) -> Result<(), Error> {
    let PairingReport { edge_kind, nodes } = report;

    let mismatch = out_halves
        .iter()
        .zip(in_halves.iter())
        .position(|(out_half, in_half)| out_half != in_half);

    // Same edge on both sides, disagreeing only about its value: the reverse half is present,
    // so reporting it as missing would send a reader looking for the wrong defect.
    if let Some(i) = mismatch {
        let (out_half, in_half) = (out_halves[i], in_halves[i]);
        if out_half.src == in_half.src && out_half.dst == in_half.dst {
            return Err(Error::edge_half_property_mismatch(
                edge_kind.as_str(),
                nodes.label(out_half.src),
                nodes.label(out_half.dst),
            ));
        }
    }

    let (half, direction) = match mismatch {
        Some(i) if out_halves[i] < in_halves[i] => (out_halves[i], Direction::Out),
        Some(i) => (in_halves[i], Direction::In),
        None => match out_halves.len().cmp(&in_halves.len()) {
            Ordering::Greater => (out_halves[in_halves.len()], Direction::Out),
            Ordering::Less => (in_halves[out_halves.len()], Direction::In),
            Ordering::Equal => return Ok(()),
        },
    };

    let (node, target) = match direction {
        Direction::Out => (half.src, half.dst),
        Direction::In => (half.dst, half.src),
    };
    Err(Error::reverse_edge_not_found(
        nodes.label(target),
        nodes.label(node),
        direction.as_str(),
        edge_kind.as_str(),
    ))
}

/// The `parallel` feature's rayon-backed driver, checking storage slots concurrently.
///
/// Every check is a pure read of a distinct slot, so the work parallelizes without
/// synchronization; only the half-edge sort and comparison need a slot's full output.
/// Per-unit results are reduced in order with `Result::and` rather than short-circuited, so a
/// corrupt graph reports the same error the sequential driver does.
#[cfg(feature = "parallel")]
mod parallel {
    use rayon::prelude::*;

    use crate::EdgeDirectionKind;

    use super::{
        Direction, Error, HalfEdge, ItemIndex, PairingReport, Schema, Storages, check_edge_slot,
        check_property_slot, report_pairing_mismatch,
    };

    /// Node-plus-half-edge count below which the sequential driver wins.
    ///
    /// Rayon's per-`join` cost dominates on small graphs, and `check_integrity` runs on graphs
    /// of a handful of nodes throughout the test suite. Measured with a single edge kind — the
    /// worst case, since more kinds split the half-edge work into more buckets and break even
    /// sooner.
    const PARALLEL_THRESHOLD: usize = 10_000;

    /// Returns whether the graph is large enough for the parallel driver to pay for itself.
    pub(super) fn is_worthwhile<S: Schema>(storages: &Storages<'_, S>) -> bool {
        let nodes: usize = storages.node_meta.iter().map(Vec::len).sum();
        let edges: usize = storages
            .edges
            .iter()
            .map(|slot| slot.neighbors().len())
            .sum();
        nodes + edges >= PARALLEL_THRESHOLD
    }

    pub(super) fn check_slots<S: Schema>(storages: &Storages<'_, S>) -> Result<(), Error> {
        let property_slots: Vec<_> = S::property_storage_slots_iter().collect();
        property_slots
            .par_iter()
            .map(|&(node_kind, property_kind)| {
                check_property_slot(storages, node_kind, property_kind)
            })
            .reduce(|| Ok(()), |a, b| a.and(b))?;

        let buckets: Vec<_> = S::edge_kinds()
            .iter()
            .flat_map(|&edge_kind| {
                Direction::values()
                    .iter()
                    .map(move |&direction| (edge_kind, direction))
            })
            .collect();

        let collected: Vec<Result<Vec<HalfEdge>, Error>> = buckets
            .par_iter()
            .map(|&(edge_kind, direction)| {
                let mut halves = Vec::new();
                for &node_kind in S::node_kinds() {
                    check_edge_slot(storages, node_kind, direction, edge_kind, &mut halves)?;
                }
                Ok(halves)
            })
            .collect();

        let mut out_halves: Vec<Vec<HalfEdge>> = vec![Vec::new(); S::number_of_edge_kinds()];
        let mut in_halves: Vec<Vec<HalfEdge>> = vec![Vec::new(); S::number_of_edge_kinds()];
        for (&(edge_kind, direction), halves) in buckets.iter().zip(collected) {
            match direction {
                Direction::Out => out_halves[edge_kind.index()] = halves?,
                Direction::In => in_halves[edge_kind.index()] = halves?,
            }
        }

        out_halves
            .par_iter_mut()
            .zip(in_halves.par_iter_mut())
            .zip(S::edge_kinds().par_iter())
            .map(|((out, incoming), &edge_kind)| {
                rayon::join(|| out.par_sort_unstable(), || incoming.par_sort_unstable());
                report_pairing_mismatch(
                    PairingReport {
                        edge_kind,
                        nodes: &storages.nodes,
                    },
                    out,
                    incoming,
                )
            })
            .reduce(|| Ok(()), |a, b| a.and(b))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::{ItemAll, ItemFromIndex, ItemFromStr, storage::StoredProperty};

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

    /// A two-enum registry, so a `RawEnumId` can name a *registered* enum other than the one a
    /// slot declares — the case that separates `EnumPropIndexMismatch` from
    /// `UnresolvedEnumKind`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestRegistry {
        Status,
        Color,
    }

    impl ItemAsStr for TestRegistry {
        fn as_str(&self) -> &'static str {
            match self {
                Self::Status => "Status",
                Self::Color => "Color",
            }
        }
    }
    impl ItemIndex for TestRegistry {
        fn index(&self) -> usize {
            match self {
                Self::Status => 0,
                Self::Color => 1,
            }
        }
    }
    impl ItemFromIndex for TestRegistry {
        fn from_index(index: usize) -> Option<Self> {
            match index {
                0 => Some(Self::Status),
                1 => Some(Self::Color),
                _ => None,
            }
        }
    }
    impl ItemAll for TestRegistry {
        fn all() -> &'static [Self] {
            &[Self::Status, Self::Color]
        }
    }
    impl FromStr for TestRegistry {
        type Err = Error;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "Status" => Ok(Self::Status),
                "Color" => Ok(Self::Color),
                _ => Err(Error::unknown_label("TestRegistry", s)),
            }
        }
    }
    impl ItemFromStr for TestRegistry {}
    impl EnumPropertyRegistry for TestRegistry {
        fn variant_count(&self) -> usize {
            match self {
                Self::Status => 2,
                Self::Color => 3,
            }
        }
    }

    #[test]
    fn enum_id_within_variant_range_is_accepted() {
        let valid = RawEnumId::new(0, 1);
        assert!(check_enum_id::<TestRegistry>(valid, None).is_ok());
    }

    #[test]
    fn enum_id_variant_out_of_range_is_rejected() {
        let out_of_range = RawEnumId::new(0, 2);
        let err = check_enum_id::<TestRegistry>(out_of_range, None).unwrap_err();
        assert!(matches!(err, Error::UnresolvedEnumVariant { .. }));
    }

    #[test]
    fn enum_id_unregistered_index_is_rejected() {
        let unregistered = RawEnumId::new(2, 0);
        let err = check_enum_id::<TestRegistry>(unregistered, None).unwrap_err();
        assert!(matches!(err, Error::UnresolvedEnumKind(_)));
    }

    #[test]
    fn enum_id_matching_the_expected_enum_is_accepted() {
        let valid = RawEnumId::new(1, 2);
        assert!(check_enum_id::<TestRegistry>(valid, Some(1)).is_ok());
    }

    #[test]
    fn enum_id_from_another_registered_enum_is_rejected() {
        let foreign = RawEnumId::new(1, 0);
        let err = check_enum_id::<TestRegistry>(foreign, Some(0)).unwrap_err();
        let Error::EnumPropIndexMismatch { expected, found } = err else {
            panic!("expected EnumPropIndexMismatch, got {err:?}");
        };
        assert_eq!(expected, "Status");
        assert_eq!(found, 1);
    }

    #[test]
    fn enum_id_from_another_registered_enum_is_accepted_without_an_expectation() {
        let foreign = RawEnumId::new(1, 0);
        assert!(check_enum_id::<TestRegistry>(foreign, None).is_ok());
    }

    #[test]
    fn enum_id_from_another_enum_reports_mismatch_before_variant_range() {
        let foreign_and_out_of_range = RawEnumId::new(1, 2);
        let err = check_enum_id::<TestRegistry>(foreign_and_out_of_range, Some(0)).unwrap_err();
        assert!(matches!(err, Error::EnumPropIndexMismatch { .. }));
    }

    fn node(seq: usize) -> RawNodeId {
        RawNodeId::new(0, seq)
    }

    fn one_kind_index() -> NodeIndex {
        NodeIndex {
            kind_offsets: vec![0, u32::MAX],
            kind_labels: vec!["Node"],
        }
    }

    fn labeled(seq: usize) -> String {
        one_kind_index().label(dense(seq))
    }

    fn dense(seq: usize) -> DenseNodeId {
        one_kind_index().densify(node(seq))
    }

    fn half(src: usize, dst: usize) -> HalfEdge {
        HalfEdge {
            src: dense(src),
            dst: dense(dst),
            value: ValueKey::default(),
        }
    }

    fn valued_half(src: usize, dst: usize, value: i32) -> HalfEdge {
        let mut values = StorageArray::new(PropertyType::Int);
        values.try_push(&StoredProperty::Int(value)).unwrap();
        HalfEdge {
            src: dense(src),
            dst: dense(dst),
            value: ValueKey::of(&values, 0),
        }
    }

    fn pair(out_halves: &mut [HalfEdge], in_halves: &mut [HalfEdge]) -> Result<(), Error> {
        check_half_edge_pairing(
            PairingReport {
                edge_kind: TestRegistry::Status,
                nodes: &one_kind_index(),
            },
            out_halves,
            in_halves,
        )
    }

    #[test]
    fn dense_node_id_round_trips_and_preserves_order() {
        let index = NodeIndex {
            kind_offsets: vec![0, 3, 5, 9],
            kind_labels: vec!["Alpha", "Beta", "Gamma"],
        };
        let ids = [
            RawNodeId::new(0, 0),
            RawNodeId::new(0, 2),
            RawNodeId::new(1, 0),
            RawNodeId::new(1, 1),
            RawNodeId::new(2, 0),
            RawNodeId::new(2, 3),
        ];
        for id in ids {
            assert_eq!(index.resolve(index.densify(id)), id, "round-trip for {id}");
        }
        let densified: Vec<_> = ids.iter().map(|&id| index.densify(id)).collect();
        assert!(
            densified.windows(2).all(|w| w[0] < w[1]),
            "dense order must match RawNodeId order: {densified:?}"
        );
    }

    #[test]
    fn node_index_is_built_from_per_kind_counts() {
        let index = NodeIndex::new([("Alpha", 3usize), ("Beta", 2)].into_iter())
            .expect("counts fit in u32");

        assert_eq!(index.kind_offsets, vec![0, 3, 5]);
        assert_eq!(index.densify(RawNodeId::new(1, 1)), DenseNodeId(4));
        assert_eq!(index.resolve(DenseNodeId(4)), RawNodeId::new(1, 1));
    }

    #[test]
    fn node_index_labels_a_node_by_kind_name_and_seq() {
        let index = NodeIndex::new([("Alpha", 3usize), ("Beta", 2)].into_iter())
            .expect("counts fit in u32");

        assert_eq!(index.label(index.densify(RawNodeId::new(0, 2))), "Alpha(2)");
        assert_eq!(index.label(index.densify(RawNodeId::new(1, 1))), "Beta(1)");
    }

    #[test]
    fn node_index_handles_empty_node_kinds() {
        let index = NodeIndex::new([("Alpha", 2usize), ("Beta", 0), ("Gamma", 2)].into_iter())
            .expect("counts fit in u32");

        assert_eq!(index.kind_offsets, vec![0, 2, 2, 4]);
        let first_of_last_kind = RawNodeId::new(2, 0);
        assert_eq!(
            index.resolve(index.densify(first_of_last_kind)),
            first_of_last_kind
        );
        assert_eq!(index.label(index.densify(first_of_last_kind)), "Gamma(0)");
    }

    #[test]
    fn node_index_rejects_a_count_beyond_u32() {
        let err = NodeIndex::new([("Alpha", u32::MAX as usize), ("Beta", 2)].into_iter())
            .expect_err("expected an overflow error");
        assert!(matches!(err, Error::NodeCountOverflow(_)));
    }

    #[test]
    fn half_edge_pairing_accepts_reordered_matching_halves() {
        let mut out_halves = [half(0, 1), half(2, 3)];
        let mut in_halves = [half(2, 3), half(0, 1)];
        assert!(pair(&mut out_halves, &mut in_halves).is_ok());
    }

    #[test]
    fn half_edge_pairing_rejects_out_half_without_reverse() {
        let mut out_halves = [half(0, 1), half(0, 2)];
        let mut in_halves = [half(0, 1)];
        let err = pair(&mut out_halves, &mut in_halves).expect_err("expected an error");

        let Error::ReverseEdgeNotFound {
            target,
            node: owner,
            direction,
            ..
        } = err
        else {
            panic!("expected ReverseEdgeNotFound, got {err:?}");
        };
        assert_eq!(target, labeled(2));
        assert_eq!(owner, labeled(0));
        assert_eq!(direction, "Out");
    }

    #[test]
    fn half_edge_pairing_rejects_in_half_without_reverse() {
        let mut out_halves = [half(0, 1)];
        let mut in_halves = [half(0, 1), half(0, 2)];
        let err = pair(&mut out_halves, &mut in_halves).expect_err("expected an error");

        let Error::ReverseEdgeNotFound {
            target,
            node: owner,
            direction,
            ..
        } = err
        else {
            panic!("expected ReverseEdgeNotFound, got {err:?}");
        };
        assert_eq!(target, labeled(0));
        assert_eq!(owner, labeled(2));
        assert_eq!(direction, "In");
    }

    #[test]
    fn half_edge_pairing_rejects_parallel_edge_count_mismatch() {
        let mut out_halves = [half(0, 1), half(0, 1)];
        let mut in_halves = [half(0, 1)];
        let err = pair(&mut out_halves, &mut in_halves).expect_err("expected an error");
        assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
    }

    #[test]
    fn half_edge_pairing_rejects_divergent_property_values() {
        let mut out_halves = [valued_half(0, 1, 7)];
        let mut in_halves = [valued_half(0, 1, 9)];
        let err = pair(&mut out_halves, &mut in_halves).expect_err("expected an error");

        let Error::EdgeHalfPropertyMismatch { src, dst, .. } = &err else {
            panic!("expected EdgeHalfPropertyMismatch, got {err:?}");
        };
        assert_eq!(*src, labeled(0));
        assert_eq!(*dst, labeled(1));
        // The message must name both storage lists, so a reader knows where to look without
        // being told the values themselves.
        let message = err.to_string();
        assert!(
            message.contains("Node(0)'s Out Status list")
                && message.contains("Node(1)'s In Status list"),
            "message should name both halves' lists, got: {message}"
        );
    }

    #[test]
    fn half_edge_pairing_accepts_equal_value_multisets_in_different_order() {
        let mut out_halves = [valued_half(0, 1, 7), valued_half(0, 1, 9)];
        let mut in_halves = [valued_half(0, 1, 9), valued_half(0, 1, 7)];
        assert!(pair(&mut out_halves, &mut in_halves).is_ok());
    }

    #[test]
    fn half_edge_pairing_rejects_parallel_edges_with_swapped_values() {
        let mut out_halves = [valued_half(0, 1, 7), valued_half(0, 1, 7)];
        let mut in_halves = [valued_half(0, 1, 7), valued_half(0, 1, 9)];
        let err = pair(&mut out_halves, &mut in_halves).expect_err("expected an error");
        assert!(matches!(err, Error::EdgeHalfPropertyMismatch { .. }));
    }

    #[test]
    fn half_edge_pairing_reports_structural_defect_over_value_when_both_differ() {
        let mut out_halves = [valued_half(0, 1, 7), valued_half(0, 2, 9)];
        let mut in_halves = [valued_half(0, 1, 7)];
        let err = pair(&mut out_halves, &mut in_halves).expect_err("expected an error");
        assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
    }
}
