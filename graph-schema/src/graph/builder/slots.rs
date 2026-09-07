use std::marker::PhantomData;

use crate::{
    ItemAsStr, ItemIndex,
    error::Error,
    node::RawNodeId,
    property::{PropertyType, PropertyValue, QuantityType},
    schema::{EdgeKind, Schema},
    storage::{
        EdgePropertyStorage, EdgeSeq, EdgeStorage, Offset, OffsetStorage, PropertyStorage,
        StorageArray,
    },
    strings_pool::StringsPool,
};

use super::QuantifiedProperty;
use super::convert::to_stored_property;
use super::staged::{
    EdgePropertyAppend, EdgeSlotInsert, EdgeSlotReplace, PropertySlotAppend, PropertySlotReplace,
};

pub(super) struct SlotBuckets<T>(Vec<Option<Vec<T>>>);

impl<T> SlotBuckets<T> {
    pub(super) fn new(slot_count: usize) -> Self {
        Self((0..slot_count).map(|_| None).collect())
    }

    pub(super) fn get(&self, slot_index: usize) -> Option<&Vec<T>> {
        self.0[slot_index].as_ref()
    }

    pub(super) fn take(&mut self, slot_index: usize) -> Option<Vec<T>> {
        self.0[slot_index].take()
    }
}

impl<T: Default> SlotBuckets<T> {
    pub(super) fn bucket(&mut self, slot_index: usize, len: usize) -> &mut Vec<T> {
        self.0[slot_index].get_or_insert_with(|| (0..len).map(|_| T::default()).collect())
    }
}

pub(super) struct SlotHalfEdge {
    pub(super) seq: usize,
    pub(super) neighbor: RawNodeId,
    pub(super) edge_seq: Option<EdgeSeq>,
}

struct SlotHalfEdges {
    halves: Vec<SlotHalfEdge>,
    counts: Vec<u32>,
}

pub(super) struct EdgeHalfBuckets(Vec<Option<SlotHalfEdges>>);

impl EdgeHalfBuckets {
    pub(super) fn new(slot_count: usize) -> Self {
        Self((0..slot_count).map(|_| None).collect())
    }

    pub(super) fn push(&mut self, slot_index: usize, node_count: usize, half: SlotHalfEdge) {
        let entry = self.0[slot_index].get_or_insert_with(|| SlotHalfEdges {
            halves: Vec::new(),
            counts: vec![0; node_count],
        });
        entry.counts[half.seq] += 1;
        entry.halves.push(half);
    }

    fn take(&mut self, slot_index: usize) -> Option<SlotHalfEdges> {
        self.0[slot_index].take()
    }
}

struct EdgeKindBatch {
    /// The next `EdgeSeq` to hand out, starting from the store's current `count`.
    next_seq: usize,
    values: StorageArray,
    /// Empty unless the kind is `Multi`; see [`EdgePropertyAppend::offsets_tail`].
    offsets_tail: Vec<Offset>,
    cumulative: Offset,
    count_delta: usize,
}

/// Accumulates the diff's new edge property values, one batch per edge kind, in `EdgeSeq`
/// order.
///
/// An `EdgeSeq` is handed out only once its edge is certain to be stored, so an edge dropped
/// for an invalid endpoint leaves no unreachable gap in the store.
pub(super) struct EdgePropertyBatches<S: Schema> {
    batches: Vec<Option<EdgeKindBatch>>,
    _phantom: PhantomData<fn() -> S>,
}

impl<S: Schema> EdgePropertyBatches<S> {
    pub(super) fn new() -> Self {
        Self {
            batches: (0..S::number_of_edge_kinds()).map(|_| None).collect(),
            _phantom: PhantomData,
        }
    }

    /// Reserves one edge's identity and appends its values, returning the `EdgeSeq` both of
    /// its halves will carry — `None` for a kind that carries no property.
    pub(super) fn push(
        &mut self,
        storage: &EdgePropertyStorage<S>,
        edge_kind: EdgeKind<S>,
        property: Option<&QuantifiedProperty>,
        strings: &mut StringsPool,
    ) -> Result<Option<EdgeSeq>, Error> {
        let property_type = S::edge_property_type(edge_kind);
        let kind_index = edge_kind.index();

        if property_type == PropertyType::None {
            // This kind stores nothing, so a supplied value would be silently dropped rather
            // than stored.
            if let Some(property) = property {
                let found = property
                    .as_slice()
                    .first()
                    .map(PropertyValue::typ)
                    .unwrap_or(PropertyType::None);
                return Err(Error::invalid_property_type(PropertyType::None, found));
            }
            return Ok(None);
        }

        let values = property.map(QuantifiedProperty::as_slice).unwrap_or(&[]);
        let is_multi = S::edge_property_quantity(edge_kind) == QuantityType::Multi;
        if !is_multi {
            // `Multi` is free to carry an empty run — that is what a CSR range of length zero
            // says — but `One` needs exactly one value: none would leave the store shorter
            // than the edges indexing it, and several would silently drop all but the first.
            match values.len() {
                1 => {}
                0 => {
                    return Err(Error::invalid_property_type(
                        property_type,
                        PropertyType::None,
                    ));
                }
                _ => return Err(Error::property_already_set(edge_kind.as_str())),
            }
        }

        let store = &storage[kind_index];
        let batch = self.batches[kind_index].get_or_insert_with(|| EdgeKindBatch {
            next_seq: store.count(),
            values: StorageArray::new(property_type),
            // A `Multi` store that has never been written needs its leading zero; one that
            // has been written already carries it.
            offsets_tail: match (is_multi, store.offsets().is_empty()) {
                (true, true) => vec![Offset::zero()],
                _ => Vec::new(),
            },
            cumulative: store.offsets().last().copied().unwrap_or_else(Offset::zero),
            count_delta: 0,
        });

        let edge_seq = EdgeSeq::new(batch.next_seq)?;
        batch.next_seq += 1;
        batch.count_delta += 1;

        for value in values {
            batch.values.try_push(&to_stored_property(value, strings))?;
        }
        if is_multi {
            batch.cumulative = batch.cumulative.checked_add_delta(values.len())?;
            batch.offsets_tail.push(batch.cumulative);
        }

        Ok(Some(edge_seq))
    }

    pub(super) fn into_appends(self) -> Vec<EdgePropertyAppend> {
        self.batches
            .into_iter()
            .enumerate()
            .filter_map(|(edge_kind_index, batch)| {
                batch.map(|batch| EdgePropertyAppend {
                    edge_kind_index,
                    values_batch: batch.values,
                    offsets_tail: batch.offsets_tail,
                    count_delta: batch.count_delta,
                })
            })
            .collect()
    }
}

// Converts `props` straight into `values` and returns `cumulative` advanced by the number
// of properties. Shared by the two call sites below that both rebuild a slot's `values`
// array while tracking a running offset alongside it.
fn append_property_batch(
    values: &mut StorageArray,
    cumulative: Offset,
    props: &[PropertyValue],
    strings: &mut StringsPool,
) -> Result<Offset, Error> {
    for prop in props {
        values.try_push(&to_stored_property(prop, strings))?;
    }
    cumulative.checked_add_delta(props.len())
}

pub(super) fn prepare_property_updates<S: Schema>(
    property_storage: &PropertyStorage<S>,
    strings: &mut StringsPool,
    mut pending: SlotBuckets<Option<&QuantifiedProperty>>,
) -> Result<Vec<PropertySlotReplace>, Error> {
    let mut replacements = Vec::new();
    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        let slot_index = S::property_storage_slot(node_kind, property_kind).index();
        let Some(seq_updates) = pending.take(slot_index) else {
            continue;
        };
        let slot = &property_storage[slot_index];

        if slot.offsets().is_empty() {
            if let Some(seq) = seq_updates.iter().position(Option::is_some) {
                return Err(Error::node_offset_not_found(seq));
            }
            continue;
        }

        let property_type = slot.values().typ();
        let mut new_values = StorageArray::with_capacity(property_type, slot.values().len());
        let mut new_offsets = Vec::with_capacity(slot.offsets().len());
        new_offsets.push(Offset::zero());

        let mut cumulative = Offset::zero();
        for end in 1..slot.offsets().len() {
            let start = end - 1;
            let orig_start = slot.offsets()[start];
            let orig_end = slot.offsets()[end];

            cumulative = if let Some(quantified_property) = seq_updates[start] {
                append_property_batch(
                    &mut new_values,
                    cumulative,
                    quantified_property.as_slice(),
                    strings,
                )?
            } else {
                for prop in slot
                    .values()
                    .iter_range(orig_start.value()..orig_end.value())
                {
                    new_values.try_push(&prop)?;
                }
                cumulative.checked_add_delta(orig_end.checked_sub(orig_start)?)?
            };

            new_offsets.push(cumulative);
        }

        replacements.push(PropertySlotReplace {
            slot_index,
            values: new_values,
            offsets: new_offsets,
        });
    }
    Ok(replacements)
}

pub(super) fn prepare_edge_removals<S: Schema>(
    edge_storage: &EdgeStorage<S>,
    mut pending: SlotBuckets<Vec<usize>>,
) -> Result<Vec<EdgeSlotReplace>, Error> {
    let mut replacements = Vec::new();
    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind).index();
        let Some(seq_removals) = pending.take(slot_index) else {
            continue;
        };
        let slot = &edge_storage[slot_index];
        if slot.offsets().is_empty() {
            continue;
        }

        let total_removals: usize = seq_removals.iter().map(Vec::len).sum();
        let mut absolute_removals: Vec<usize> = Vec::with_capacity(total_removals);
        for (start, local_seqs) in seq_removals.iter().enumerate() {
            if local_seqs.is_empty() {
                continue;
            }
            let (orig_start, orig_end) = slot
                .get_offset(start)
                .ok_or_else(|| Error::node_offset_not_found(start))?;
            let degree = orig_end.checked_sub(orig_start)?;
            for &local_seq in local_seqs {
                if local_seq >= degree {
                    return Err(Error::node_offset_not_found(local_seq));
                }
                absolute_removals.push(orig_start.value() + local_seq);
            }
        }
        if absolute_removals.is_empty() {
            continue;
        }
        absolute_removals.sort_unstable();

        let kept = slot.neighbors().len() - absolute_removals.len();
        let mut removal_iter = absolute_removals.iter().copied().peekable();
        let mut new_neighbors = Vec::with_capacity(kept);
        // Empty for edge kinds carrying no property, which hold no `EdgeSeq`; `get` below
        // then yields nothing and leaves it that way.
        let mut new_edges = Vec::with_capacity(if slot.edges().is_empty() { 0 } else { kept });
        for (i, &neighbor) in slot.neighbors().iter().enumerate() {
            if removal_iter.peek() == Some(&i) {
                removal_iter.next();
                continue;
            }
            new_neighbors.push(neighbor);
            if let Some(&edge_seq) = slot.edges().get(i) {
                new_edges.push(edge_seq);
            }
        }

        // Per-node removal counts, not `absolute_removals`, drive the shift here: each
        // node's own count from `seq_removals` is already exactly how much every offset
        // from that node onward must decrease by, with no need to re-derive it from the
        // sorted absolute-position array the filter pass above used.
        let mut new_offsets = slot.offsets().clone();
        let mut removed_so_far = 0usize;
        for end in 1..new_offsets.len() {
            removed_so_far += seq_removals[end - 1].len();
            new_offsets[end] = new_offsets[end].checked_sub_delta(removed_so_far)?;
        }

        replacements.push(EdgeSlotReplace {
            slot_index,
            neighbors: new_neighbors,
            edges: new_edges,
            offsets: new_offsets,
        });
    }
    Ok(replacements)
}

pub(super) fn prepare_new_node_properties<S: Schema>(
    property_storage: &PropertyStorage<S>,
    strings: &mut StringsPool,
    graph_nodes_max_seq: &[usize],
    new_nodes_count: &[usize],
    mut slot_property: SlotBuckets<Option<&Vec<PropertyValue>>>,
    property_last_offset: &[Option<Offset>],
) -> Result<Vec<PropertySlotAppend>, Error> {
    let mut appends = Vec::new();
    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        let nodes_count = new_nodes_count[node_kind.index()];
        if nodes_count == 0 {
            continue;
        }

        let slot_index = S::property_storage_slot(node_kind, property_kind).index();
        let slot = &property_storage[slot_index];
        let baseline_last = property_last_offset[slot_index];

        let Some(seq_property) = slot_property.take(slot_index) else {
            let Some(last) = baseline_last.or_else(|| slot.offsets().last().copied()) else {
                continue;
            };
            appends.push(PropertySlotAppend {
                slot_index,
                values_batch: StorageArray::with_capacity(slot.values().typ(), 0),
                offsets_tail: vec![last; nodes_count],
            });
            continue;
        };

        // When set, this is also how many leading zero-offsets get backfilled below, so it's
        // folded into `offsets_tail`'s capacity up front instead of letting that first `extend`
        // force a reallocation.
        let backfill_len = if baseline_last.is_none() && slot.offsets().is_empty() {
            graph_nodes_max_seq[node_kind.index()] + 1
        } else {
            0
        };
        let mut offsets_tail = Vec::with_capacity(nodes_count + backfill_len);
        let mut cumulative = match baseline_last {
            Some(last) => last,
            None if slot.offsets().is_empty() => {
                offsets_tail.extend(std::iter::repeat_n(Offset::zero(), backfill_len));
                Offset::zero()
            }
            None => slot.offsets().last().copied().unwrap_or_else(Offset::zero),
        };

        let mut values_batch = StorageArray::with_capacity(slot.values().typ(), nodes_count);
        for local_index in 0..nodes_count {
            if let Some(props) = seq_property.get(local_index).copied().flatten() {
                cumulative = append_property_batch(&mut values_batch, cumulative, props, strings)?;
            }
            offsets_tail.push(cumulative);
        }

        appends.push(PropertySlotAppend {
            slot_index,
            values_batch,
            offsets_tail,
        });
    }
    Ok(appends)
}

pub(super) fn prepare_new_edges<S: Schema>(
    edge_storage: &EdgeStorage<S>,
    graph_nodes_max_seq: &[usize],
    new_nodes_count: &[usize],
    mut slot_edge_halves: EdgeHalfBuckets,
    edge_offsets_baseline: &[Option<&Vec<Offset>>],
) -> Result<Vec<EdgeSlotInsert>, Error> {
    let mut inserts = Vec::new();
    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind).index();
        let nodes_count = new_nodes_count[node_kind.index()];
        let seq_halves = slot_edge_halves.take(slot_index);
        if nodes_count == 0 && seq_halves.is_none() {
            continue;
        }

        let slot = &edge_storage[slot_index];
        let offsets_len = graph_nodes_max_seq[node_kind.index()] + nodes_count + 1;

        let mut offsets: Vec<Offset> = match edge_offsets_baseline[slot_index] {
            Some(baseline) => baseline.clone(),
            None => slot.offsets().clone(),
        };

        if offsets.is_empty() {
            if seq_halves.is_none() {
                continue;
            }
            offsets = vec![Offset::zero(); offsets_len];
        } else if offsets.len() != offsets_len {
            let last = offsets.last().copied().unwrap_or_else(Offset::zero);
            offsets.resize(offsets_len, last);
        }

        let Some(seq_halves) = seq_halves else {
            inserts.push(EdgeSlotInsert {
                slot_index,
                neighbors: Vec::new(),
                edges: Vec::new(),
                batches: Vec::new(),
                offsets,
            });
            continue;
        };

        let SlotHalfEdges { halves, mut counts } = seq_halves;
        // Schema-derived rather than read off the halves: it is the same fact `prepare` used
        // when it decided whether to hand this kind's edges an `EdgeSeq` at all.
        let has_edges = S::edge_property_type(edge_kind) != PropertyType::None;
        debug_assert_eq!(counts.len(), offsets.len() - 1);

        // Guards every `as u32` below: the histogram and its prefix sums are all bounded
        // by the total number of halves.
        u32::try_from(halves.len()).map_err(|_| Error::offset_overflow(halves.len()))?;

        let mut acc = 0u32;
        let mut touched = 0usize;
        for count in counts.iter_mut() {
            let degree = *count;
            if degree > 0 {
                touched += 1;
            }
            *count = acc;
            acc += degree;
        }

        let mut order = vec![0u32; halves.len()];
        for (i, half) in halves.iter().enumerate() {
            let cursor = &mut counts[half.seq];
            order[*cursor as usize] = i as u32;
            *cursor += 1;
        }

        let mut neighbors: Vec<RawNodeId> = Vec::with_capacity(halves.len());
        let mut edges: Vec<EdgeSeq> = Vec::with_capacity(if has_edges { halves.len() } else { 0 });
        for &i in &order {
            let half = &halves[i as usize];
            neighbors.push(half.neighbor);
            // A half without an `EdgeSeq` must not be skipped on a kind that has them: it
            // counts as a neighbor either way, so skipping would leave `edges` shorter than
            // `neighbors` and shift every later edge's identity onto the wrong edge.
            // `EdgePropertyBatches::push` already rejected the mismatched cases, so this only
            // records what it decided.
            debug_assert_eq!(half.edge_seq.is_some(), has_edges);
            if let Some(edge_seq) = half.edge_seq {
                edges.push(edge_seq);
            }
        }

        let mut batches: Vec<(usize, usize)> = Vec::with_capacity(touched);
        let mut prev_end = 0usize;
        for (count, offset) in counts.iter().zip(offsets.iter_mut().skip(1)) {
            let run_end = *count as usize;
            if run_end > prev_end {
                batches.push((offset.value() + prev_end, run_end - prev_end));
            }
            *offset = offset.checked_add_delta(run_end)?;
            prev_end = run_end;
        }

        debug_assert_eq!(prev_end, neighbors.len());

        inserts.push(EdgeSlotInsert {
            slot_index,
            neighbors,
            edges,
            batches,
            offsets,
        });
    }
    Ok(inserts)
}
