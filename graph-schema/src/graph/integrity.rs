//! Integrity checking for a graph's flat CSR storage.
//!
//! [`CheckIntegrity::check_integrity`] checks that the graph's storage is valid. Normally
//! [`crate::graph::builder::GraphDiff::apply`] keeps this true step by step as it builds the
//! graph. This check verifies the same rules directly on the final data: offset arrays are
//! well-formed, storage slot types match the schema, node/string/enum references point to real
//! data, and every edge has exactly one half-edge per direction joining the same two nodes.
//! `TryFrom<RawGraph<S>> for Graph<S>` runs this check before returning a valid
//! [`Graph<S>`](crate::graph::Graph).
//!
//! How half-edges pair up depends on whether their kind allocates identities:
//! - A kind carrying a property gives each of its edges an [`EdgeSeq`], stored on both halves.
//!   Pairing scatters the halves by that seq and requires exactly one `Out` and one `In` half
//!   per seq, agreeing on the two nodes they join. No property value is compared: it is stored
//!   once, so the halves cannot disagree about it.
//! - A kind typed [`PropertyType::None`] allocates no `EdgeSeq` — its edges carry no data and
//!   parallel ones are interchangeable — so pairing is endpoint-and-degree symmetry over sorted
//!   multisets, which also catches a mismatched count of parallel edges.
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
    property::{PropertyType, QuantityType},
    schema::Schema,
    storage::{
        EdgePropertyStorage, EdgeSeq, EdgeStorage, NodeMetaStorage, Offset, OffsetStorage,
        PropertyStorage, StorageArray,
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

/// A half-edge canonicalized as a `(source, destination)` pair, so that the two halves of one
/// edge produce the same pair whichever endpoint they are stored on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct HalfEdge {
    src: DenseNodeId,
    dst: DenseNodeId,
}

impl HalfEdge {
    /// Marks a seq no half-edge claimed. Using a sentinel rather than `Option` keeps the
    /// scatter arrays at 8 bytes per edge; `DenseNodeId(u32::MAX)` cannot name a real node,
    /// since [`NodeIndex::new`] rejects a graph whose node count reaches `u32::MAX`.
    const ABSENT: Self = Self {
        src: DenseNodeId(u32::MAX),
        dst: DenseNodeId(u32::MAX),
    };
}

/// The half-edges one edge kind's slots contribute to the pairing check, in one of two forms.
///
/// Exactly one is populated, decided by whether the kind allocates [`EdgeSeq`]s — every slot
/// of a kind agrees on that, because it follows from the schema alone.
#[derive(Default, Clone)]
struct KindHalves {
    /// `(identity, endpoints)` for kinds that allocate identities; paired by scattering.
    identified: Vec<(EdgeSeq, HalfEdge)>,
    /// Endpoints alone, for kinds that do not; paired by sorted multiset.
    anonymous: Vec<HalfEdge>,
}

impl KindHalves {
    fn reserve(&mut self, additional: usize, identified: bool) {
        if identified {
            self.identified.reserve(additional);
        } else {
            self.anonymous.reserve(additional);
        }
    }
}

struct Storages<'a, S: Schema> {
    node_meta: &'a NodeMetaStorage<S>,
    edges: &'a EdgeStorage<S>,
    properties: &'a PropertyStorage<S>,
    edge_properties: &'a EdgePropertyStorage<S>,
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
    edge_property_storage: &EdgePropertyStorage<S>,
    strings: &StringsPool,
) -> Result<(), Error> {
    check_storage_sizes::<S>(
        node_meta_storage,
        edge_storage,
        property_storage,
        edge_property_storage,
    )?;

    let storages = Storages {
        node_meta: node_meta_storage,
        edges: edge_storage,
        properties: property_storage,
        edge_properties: edge_property_storage,
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

    for &edge_kind in S::edge_kinds() {
        check_edge_property_store(storages, edge_kind)?;
    }

    let mut out_halves = vec![KindHalves::default(); S::number_of_edge_kinds()];
    let mut in_halves = vec![KindHalves::default(); S::number_of_edge_kinds()];

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
            storages.edge_properties[edge_kind.index()].count(),
            &mut out_halves[edge_kind.index()],
            &mut in_halves[edge_kind.index()],
        )?;
    }

    Ok(())
}

/// Checks one edge kind's property store: its column's type, the shape its quantity implies,
/// and the node/string/enum ids its values embed.
fn check_edge_property_store<S: Schema>(
    storages: &Storages<'_, S>,
    edge_kind: S::E,
) -> Result<(), Error> {
    let label = format!("EdgePropertyStore({})", edge_kind.as_str());
    let store = &storages.edge_properties[edge_kind.index()];
    let expected_type = S::edge_property_type(edge_kind);
    check_storage_type(store.values(), expected_type)?;

    if expected_type == PropertyType::None {
        // Such a kind stores nothing and hands out no identity, so nothing may be there:
        // a non-zero count would leave `EdgeSeq`s no half-edge can legally carry.
        if store.count() != 0 {
            return Err(Error::offsets_bounds_mismatch(label, 0, store.count()));
        }
        if !store.offsets().is_empty() {
            return Err(Error::offsets_length_mismatch(
                label,
                0,
                store.offsets().len(),
            ));
        }
        return Ok(());
    }

    if S::edge_property_quantity(edge_kind) == QuantityType::Multi {
        // `check_offsets_shape` waves an empty array through as "slot unused", which is only
        // true here while no edge of the kind exists.
        if store.offsets().is_empty() && store.count() != 0 {
            return Err(Error::offsets_length_mismatch(label, store.count() + 1, 0));
        }
        check_offsets_shape(&label, store.offsets(), store.count())?;
        check_offsets_bounds(&label, store.offsets(), store.values().len())?;
    } else {
        // `One` lets an `EdgeSeq` index `values` directly, so it keeps no offsets at all and
        // the column holds exactly one value per edge handed an identity.
        if !store.offsets().is_empty() {
            return Err(Error::offsets_length_mismatch(
                label,
                0,
                store.offsets().len(),
            ));
        }
        if store.values().len() != store.count() {
            return Err(Error::offsets_bounds_mismatch(
                label,
                store.count(),
                store.values().len(),
            ));
        }
    }

    check_values_content::<S>(
        store.values(),
        storages.node_meta,
        storages.strings,
        S::edge_property_enum_index(edge_kind),
    )
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
    halves: &mut KindHalves,
) -> Result<(), Error> {
    let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind);
    let expected_count = storages.node_meta[node_kind.index()].len();

    let slot = &storages.edges[slot_index.index()];
    check_offsets_shape(slot_index, slot.offsets(), expected_count)?;

    check_offsets_bounds(slot_index, slot.offsets(), slot.neighbors().len())?;
    for node_id in slot.neighbors() {
        check_node_id::<S>(*node_id, storages.node_meta)?;
    }

    // A kind carrying a property gives every one of its half-edges an `EdgeSeq`; one typed
    // `PropertyType::None` gives none. Anything between would desynchronize `edges` from
    // `neighbors` and shift identities onto the wrong edges.
    let identified = S::edge_property_type(edge_kind) != PropertyType::None;
    let expected_edges = if identified {
        slot.neighbors().len()
    } else {
        0
    };
    if slot.edges().len() != expected_edges {
        return Err(Error::edge_seq_length_mismatch(
            slot_index.to_string(),
            expected_edges,
            slot.edges().len(),
        ));
    }

    let count = storages.edge_properties[edge_kind.index()].count();
    for &edge_seq in slot.edges() {
        if edge_seq.index() >= count {
            return Err(Error::edge_seq_out_of_bounds(
                edge_kind.as_str(),
                edge_seq.index(),
                count,
            ));
        }
    }

    halves.reserve(slot.neighbors().len(), identified);
    for (seq, window) in slot.offsets().windows(2).enumerate() {
        let node = RawNodeId::new(node_kind.index(), seq);
        let start = window[0].value();
        let node = storages.nodes.densify(node);
        for (offset, neighbor) in slot.get_neighbors(window[0], window[1]).enumerate() {
            let neighbor = storages.nodes.densify(neighbor);
            let (src, dst) = match direction {
                Direction::Out => (node, neighbor),
                Direction::In => (neighbor, node),
            };
            let ends = HalfEdge { src, dst };
            // The length check above pins which arm this takes for every half of the slot.
            match slot.edges().get(start + offset) {
                Some(&edge_seq) => halves.identified.push((edge_seq, ends)),
                None => halves.anonymous.push(ends),
            }
        }
    }

    Ok(())
}

fn check_storage_sizes<S: Schema>(
    node_meta_storage: &NodeMetaStorage<S>,
    edge_storage: &EdgeStorage<S>,
    property_storage: &PropertyStorage<S>,
    edge_property_storage: &EdgePropertyStorage<S>,
) -> Result<(), Error> {
    if edge_property_storage.len() != S::number_of_edge_kinds() {
        return Err(Error::storage_size_mismatch(
            "edge_property_storage",
            S::number_of_edge_kinds(),
            edge_property_storage.len(),
        ));
    }
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

/// Confirms every half-edge of one edge kind has a matching reverse half.
///
/// Which of the two strategies applies follows from the schema, so the populated arm of
/// [`KindHalves`] decides it: identities where the kind has them, sorted multisets where it
/// does not.
fn check_half_edge_pairing<E: ItemAsStr + Copy>(
    report: PairingReport<'_, E>,
    count: usize,
    out_halves: &mut KindHalves,
    in_halves: &mut KindHalves,
) -> Result<(), Error> {
    if out_halves.anonymous.is_empty() && in_halves.anonymous.is_empty() {
        return check_pairing_by_edge_seq(
            report,
            count,
            &out_halves.identified,
            &in_halves.identified,
        );
    }
    out_halves.anonymous.sort_unstable();
    in_halves.anonymous.sort_unstable();
    report_pairing_mismatch(report, &out_halves.anonymous, &in_halves.anonymous)
}

/// Pairs an edge kind's halves by the identity they share.
///
/// Each `EdgeSeq` must be claimed by exactly one `Out` and one `In` half, and both must
/// canonicalize to the same `(source, destination)`. A seq claimed by neither is an edge
/// removed since the last compaction, which is not a defect. Scattering by seq costs one pass
/// and `count` entries instead of sorting both sides, and needs no value comparison at all:
/// the property lives in one place, so there are no two copies to disagree.
fn check_pairing_by_edge_seq<E: ItemAsStr + Copy>(
    report: PairingReport<'_, E>,
    count: usize,
    out_halves: &[(EdgeSeq, HalfEdge)],
    in_halves: &[(EdgeSeq, HalfEdge)],
) -> Result<(), Error> {
    let PairingReport { edge_kind, nodes } = report;

    let mut out_ends = vec![HalfEdge::ABSENT; count];
    let mut in_ends = vec![HalfEdge::ABSENT; count];
    scatter_halves(edge_kind, Direction::Out, out_halves, &mut out_ends)?;
    scatter_halves(edge_kind, Direction::In, in_halves, &mut in_ends)?;

    for (out_half, in_half) in out_ends.iter().zip(in_ends.iter()) {
        if out_half == in_half {
            // Either both halves agree, or neither exists and the seq is dead storage.
            continue;
        }

        // Whichever half is present is the one whose reverse is missing or points elsewhere;
        // when both are present but disagree, the `Out` side names the defect.
        let (half, direction) = if *out_half != HalfEdge::ABSENT {
            (*out_half, Direction::Out)
        } else {
            (*in_half, Direction::In)
        };
        let (node, target) = match direction {
            Direction::Out => (half.src, half.dst),
            Direction::In => (half.dst, half.src),
        };
        return Err(Error::reverse_edge_not_found(
            nodes.label(target),
            nodes.label(node),
            direction.as_str(),
            edge_kind.as_str(),
        ));
    }

    Ok(())
}

/// Places each half at its own seq, rejecting a seq two halves of the same direction claim.
fn scatter_halves<E: ItemAsStr>(
    edge_kind: E,
    direction: Direction,
    halves: &[(EdgeSeq, HalfEdge)],
    ends: &mut [HalfEdge],
) -> Result<(), Error> {
    for &(edge_seq, half) in halves {
        // In range because `check_edge_slot` bounds every `EdgeSeq` against the same count
        // these arrays were sized from.
        let slot = &mut ends[edge_seq.index()];
        if *slot != HalfEdge::ABSENT {
            return Err(Error::duplicate_half_edge(
                edge_kind.as_str(),
                edge_seq.index(),
                direction.as_str(),
            ));
        }
        *slot = half;
    }
    Ok(())
}

struct PairingReport<'a, E> {
    edge_kind: E,
    nodes: &'a NodeIndex,
}

/// Reports the lowest-ordered half-edge that has no matching reverse, given both sides already
/// sorted.
///
/// Comparing multisets rather than testing each half for the mere existence of a reverse
/// catches a mismatched count of parallel edges: two `Out` edges from A to B against a single
/// `In` edge back from B to A does have a matching reverse edge — just not enough of them.
/// Only kinds without [`EdgeSeq`]s reach here; the rest pair by identity, where the count
/// follows from each seq holding exactly one half per direction.
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
        Direction, Error, ItemIndex, KindHalves, PairingReport, Schema, Storages,
        check_edge_property_store, check_edge_slot, check_half_edge_pairing, check_property_slot,
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

        S::edge_kinds()
            .par_iter()
            .map(|&edge_kind| check_edge_property_store(storages, edge_kind))
            .reduce(|| Ok(()), |a, b| a.and(b))?;

        let buckets: Vec<_> = S::edge_kinds()
            .iter()
            .flat_map(|&edge_kind| {
                Direction::values()
                    .iter()
                    .map(move |&direction| (edge_kind, direction))
            })
            .collect();

        let collected: Vec<Result<KindHalves, Error>> = buckets
            .par_iter()
            .map(|&(edge_kind, direction)| {
                let mut halves = KindHalves::default();
                for &node_kind in S::node_kinds() {
                    check_edge_slot(storages, node_kind, direction, edge_kind, &mut halves)?;
                }
                Ok(halves)
            })
            .collect();

        let mut out_halves = vec![KindHalves::default(); S::number_of_edge_kinds()];
        let mut in_halves = vec![KindHalves::default(); S::number_of_edge_kinds()];
        for (&(edge_kind, direction), halves) in buckets.iter().zip(collected) {
            match direction {
                Direction::Out => out_halves[edge_kind.index()] = halves?,
                Direction::In => in_halves[edge_kind.index()] = halves?,
            }
        }

        // Scattering by `EdgeSeq` writes into one array per kind, so it stays inside this
        // per-kind unit of work rather than being split further; the sorted path for kinds
        // without identities parallelizes as it did.
        out_halves
            .par_iter_mut()
            .zip(in_halves.par_iter_mut())
            .zip(S::edge_kinds().par_iter())
            .map(|((out, incoming), &edge_kind)| {
                check_half_edge_pairing(
                    PairingReport {
                        edge_kind,
                        nodes: &storages.nodes,
                    },
                    storages.edge_properties[edge_kind.index()].count(),
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
        }
    }

    /// Pairs halves of a kind that allocates no `EdgeSeq`, by sorted multiset.
    fn pair(out_halves: &[HalfEdge], in_halves: &[HalfEdge]) -> Result<(), Error> {
        let mut out = KindHalves {
            anonymous: out_halves.to_vec(),
            ..KindHalves::default()
        };
        let mut incoming = KindHalves {
            anonymous: in_halves.to_vec(),
            ..KindHalves::default()
        };
        check_half_edge_pairing(report(), 0, &mut out, &mut incoming)
    }

    /// Pairs halves of a kind that does, by the identity each claims.
    fn pair_by_seq(
        count: usize,
        out_halves: &[(usize, HalfEdge)],
        in_halves: &[(usize, HalfEdge)],
    ) -> Result<(), Error> {
        let identify = |halves: &[(usize, HalfEdge)]| {
            halves
                .iter()
                .map(|&(seq, ends)| (EdgeSeq::new(seq).expect("seq fits in u32"), ends))
                .collect::<Vec<_>>()
        };
        let mut out = KindHalves {
            identified: identify(out_halves),
            ..KindHalves::default()
        };
        let mut incoming = KindHalves {
            identified: identify(in_halves),
            ..KindHalves::default()
        };
        check_half_edge_pairing(report(), count, &mut out, &mut incoming)
    }

    fn report() -> PairingReport<'static, TestRegistry> {
        // Leaked so the helpers above can hand back a `'static` report without threading a
        // borrow through every call; one small allocation per test run.
        PairingReport {
            edge_kind: TestRegistry::Status,
            nodes: Box::leak(Box::new(one_kind_index())),
        }
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
        assert!(pair(&[half(0, 1), half(2, 3)], &[half(2, 3), half(0, 1)]).is_ok());
    }

    #[test]
    fn half_edge_pairing_rejects_out_half_without_reverse() {
        let err = pair(&[half(0, 1), half(0, 2)], &[half(0, 1)]).expect_err("expected an error");

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
        let err = pair(&[half(0, 1)], &[half(0, 1), half(0, 2)]).expect_err("expected an error");

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

    /// Without identities, parallel edges are told apart only by counting: two `Out` halves
    /// against one `In` half does have a matching reverse — just not enough of them.
    #[test]
    fn half_edge_pairing_rejects_parallel_edge_count_mismatch() {
        let err = pair(&[half(0, 1), half(0, 1)], &[half(0, 1)]).expect_err("expected an error");
        assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
    }

    #[test]
    fn seq_pairing_accepts_halves_claiming_the_same_seq() {
        assert!(
            pair_by_seq(
                2,
                &[(0, half(0, 1)), (1, half(2, 3))],
                &[(1, half(2, 3)), (0, half(0, 1))]
            )
            .is_ok()
        );
    }

    /// A seq no half claims is an edge removed since the last compaction, not a defect.
    #[test]
    fn seq_pairing_accepts_a_seq_no_half_claims() {
        assert!(pair_by_seq(3, &[(0, half(0, 1))], &[(0, half(0, 1))]).is_ok());
    }

    #[test]
    fn seq_pairing_rejects_a_seq_with_only_one_half() {
        let err = pair_by_seq(2, &[(0, half(0, 1)), (1, half(0, 2))], &[(0, half(0, 1))])
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
        assert_eq!(target, labeled(2));
        assert_eq!(owner, labeled(0));
        assert_eq!(direction, "Out");
    }

    /// Both halves exist and claim the same edge, but disagree about which nodes it joins.
    #[test]
    fn seq_pairing_rejects_halves_of_one_seq_joining_different_nodes() {
        let err =
            pair_by_seq(1, &[(0, half(0, 1))], &[(0, half(0, 2))]).expect_err("expected an error");
        assert!(matches!(err, Error::ReverseEdgeNotFound { .. }));
    }

    /// Two halves of the same direction on one seq would make the edge's degree wrong on that
    /// side while still leaving every seq paired, so it needs its own rejection.
    #[test]
    fn seq_pairing_rejects_a_seq_claimed_twice_in_one_direction() {
        let err = pair_by_seq(1, &[(0, half(0, 1)), (0, half(0, 1))], &[(0, half(0, 1))])
            .expect_err("expected an error");

        let Error::DuplicateHalfEdge {
            edge_seq,
            direction,
            ..
        } = err
        else {
            panic!("expected DuplicateHalfEdge, got {err:?}");
        };
        assert_eq!(edge_seq, 0);
        assert_eq!(direction, "Out");
    }
}
