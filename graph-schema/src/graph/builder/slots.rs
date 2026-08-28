use crate::{
    ItemIndex,
    error::Error,
    node::RawNodeId,
    property::{PropertyType, PropertyValue},
    schema::Schema,
    storage::{EdgeStorage, Offset, OffsetStorage, PropertyStorage, StorageArray},
    strings_pool::StringsPool,
};

use super::convert::stored_property_batch;
use super::staged::{EdgeSlotInsert, EdgeSlotReplace, PropertySlotAppend, PropertySlotReplace};
use super::{HalfEdge, QuantifiedProperty};

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

// Converts `props` to stored form, appends the batch to `values`, and returns `cumulative`
// advanced by the batch's length. Shared by the two call sites below that both rebuild a
// slot's `values` array while tracking a running offset alongside it.
fn append_property_batch(
    values: &mut StorageArray,
    cumulative: Offset,
    props: &[PropertyValue],
    typ: PropertyType,
    strings: &mut StringsPool,
) -> Result<Offset, Error> {
    let mut batch = stored_property_batch(props, typ, strings)?;
    let cumulative = cumulative.checked_add_delta(batch.len())?;
    values.try_append(&mut batch)?;
    Ok(cumulative)
}

// Prepares every pending `UpdateNodeProperty` for one slot by rebuilding its `values`
// array in a single pass — copying each untouched node's existing span across and
// substituting each touched node's new batch — instead of draining and splicing the
// shared array once per touched node, which would cost O(suffix length) per node and
// reintroduce this rewrite's target complexity (O(n·k) for k touched nodes in an
// n-node slot). Reads `property_storage`'s pre-diff state only; performs no mutation.
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
                let new_node_values: &[PropertyValue] = match quantified_property {
                    QuantifiedProperty::One(p) => std::slice::from_ref(p),
                    QuantifiedProperty::Multi(ps) => ps.as_slice(),
                };
                append_property_batch(
                    &mut new_values,
                    cumulative,
                    new_node_values,
                    property_type,
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

// Prepares every pending `RemoveEdge` half in one pass per touched slot: local seqs
// are first converted to absolute positions against the slot's original (pre-diff)
// offsets, then `neighbors`/`values` are rebuilt by filtering out those positions in
// a single O(slot size) pass, and offsets are shifted by a cumulative "removed so
// far" count in a second pass. Reads `edge_storage`'s pre-diff state only; performs
// no mutation.
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

        let property_type = slot.values().typ();
        let mut removal_iter = absolute_removals.iter().copied().peekable();
        let mut new_neighbors =
            Vec::with_capacity(slot.neighbors().len() - absolute_removals.len());
        let mut new_values = StorageArray::with_capacity(
            property_type,
            slot.values().len().saturating_sub(absolute_removals.len()),
        );
        for (i, &neighbor) in slot.neighbors().iter().enumerate() {
            if removal_iter.peek() == Some(&i) {
                removal_iter.next();
                continue;
            }
            new_neighbors.push(neighbor);
            if let Some(v) = slot.values().get(i) {
                new_values.try_push(&v)?;
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
            values: new_values,
            offsets: new_offsets,
        });
    }
    Ok(replacements)
}

// Prepares the property/offset append batch for every property slot touched by
// brand-new nodes. `property_last_offset[slot_index]`, when set, means this slot was
// also rebuilt by `prepare_property_updates` in the same diff — its final offset is
// used as the starting point here instead of `property_storage`'s pre-diff state, so
// the append lands after whatever `commit` will have already written for that slot's
// existing nodes (see coupling note in `GraphDiff::prepare`).
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

        let mut values_batch = StorageArray::with_capacity(slot.values().typ(), 0);
        for local_index in 0..nodes_count {
            if let Some(props) = seq_property.get(local_index).copied().flatten() {
                cumulative = append_property_batch(
                    &mut values_batch,
                    cumulative,
                    props,
                    slot.values().typ(),
                    strings,
                )?;
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

// Prepares the neighbor/value/offset insertion for every edge slot touched by a new
// edge (connecting any combination of new and existing nodes) or by new nodes needing
// degree-0 offset placeholders. `edge_offsets_baseline[slot_index]`, when set, means
// this slot was also rebuilt by `prepare_edge_removals` in the same diff — its final
// offsets are used as the starting point here instead of `edge_storage`'s pre-diff
// state (same coupling reason as `prepare_new_node_properties`).
pub(super) fn prepare_new_edges<S: Schema>(
    edge_storage: &EdgeStorage<S>,
    graph_nodes_max_seq: &[usize],
    new_nodes_count: &[usize],
    mut slot_edge_halves: SlotBuckets<Vec<HalfEdge<S>>>,
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
                splices: Vec::new(),
                offsets,
            });
            continue;
        };

        let mut splices = Vec::new();
        let mut delta = 0usize;

        #[allow(clippy::needless_range_loop)]
        for end in 1..offsets.len() {
            let start = end - 1;

            let halves = &seq_halves[start];
            if !halves.is_empty() {
                let neighbors: Vec<RawNodeId> = halves
                    .iter()
                    .map(|h| RawNodeId::from(&h.neighbor))
                    .collect();
                let place = offsets[end].value() + delta;
                delta += halves.len();

                let property_type = slot.values().typ();
                let values = if property_type != PropertyType::None {
                    let mut batch = StorageArray::with_capacity(property_type, halves.len());
                    for half in halves.iter() {
                        if let Some(prop) = &half.property {
                            batch.try_push(prop)?;
                        }
                    }
                    Some(batch)
                } else {
                    None
                };
                splices.push((place, neighbors, values));
            }

            offsets[end] = offsets[end].checked_add_delta(delta)?;
        }

        inserts.push(EdgeSlotInsert {
            slot_index,
            splices,
            offsets,
        });
    }
    Ok(inserts)
}
