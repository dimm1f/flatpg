//! Reclaiming the edge property storage that removed edges leave behind.
//!
//! The write path is append-only: [`GraphDiff`](crate::graph::builder::GraphDiff) hands every
//! new edge a fresh [`EdgeSeq`] and never reuses one, so removing an edge drops its two halves
//! but leaves its values in the kind's store. [`Graph::compact_edge_properties`] is the only
//! thing that reclaims them.

use crate::{
    ItemAsStr, ItemIndex,
    error::Error,
    graph::Graph,
    property::{PropertyType, QuantityType},
    schema::Schema,
    storage::{EdgePropertyStore, EdgeSeq, EdgeStorage, Offset, OffsetStorage, StorageArray},
};

/// Marks a seq no half-edge references any more, in the remap table below.
///
/// Sound as a sentinel because a live seq is always `< count` and `count` never reaches
/// `u32::MAX` — [`EdgeSeq::new`] rejects a count that does not fit in a `u32`.
const DEAD: u32 = u32::MAX;

impl<S: Schema> Graph<S> {
    /// Reclaims the property storage of edges removed since the last compaction, renumbering
    /// every [`EdgeSeq`].
    ///
    /// A seq is dead exactly when no half-edge references it, so this needs no tombstone of
    /// its own: the halves are the only thing keeping a seq alive, and removing an edge drops
    /// both. Costs one pass over every half-edge plus one over the values, and does nothing
    /// for kinds that carry no property — those hand out no `EdgeSeq` to begin with.
    ///
    /// # Invalidates
    ///
    /// Every [`EdgeId`](crate::edge::EdgeId) obtained before this call: an edge's identity is
    /// its position in its kind's store, and compaction moves it. Re-read the edges you need
    /// afterwards rather than carrying ids across.
    pub fn compact_edge_properties(&mut self) -> Result<(), Error> {
        for &edge_kind in S::edge_kinds() {
            if S::edge_property_type(edge_kind) == PropertyType::None {
                continue;
            }

            let kind_index = edge_kind.index();
            let count = self.edge_property_storage[kind_index].count();
            if count == 0 {
                continue;
            }

            let remap = live_seq_remap::<S>(&self.edge_storage, edge_kind, count);
            let live_count = remap.iter().filter(|&&seq| seq != DEAD).count();
            if live_count == count {
                // Nothing died, so every seq would map to itself; leave both the store and the
                // half-edges untouched rather than rewriting them into the same values.
                continue;
            }

            let is_multi = S::edge_property_quantity(edge_kind) == QuantityType::Multi;
            rebuild_store(
                &mut self.edge_property_storage[kind_index],
                &remap,
                live_count,
                is_multi,
            )?;

            for (node_kind, direction, kind) in S::edge_storage_slots_iter() {
                if kind != edge_kind {
                    continue;
                }
                let slot_index = S::edge_storage_slot(node_kind, direction, kind).index();
                for edge_seq in self.edge_storage[slot_index].edges_mut() {
                    // In range, and live: `remap` was built by walking these very arrays, so
                    // every seq they hold was marked before the prefix sum ran.
                    let new_seq = remap
                        .get(edge_seq.index())
                        .copied()
                        .filter(|&seq| seq != DEAD)
                        .ok_or_else(|| {
                            Error::edge_seq_out_of_bounds(
                                edge_kind.as_str(),
                                edge_seq.index(),
                                count,
                            )
                        })?;
                    *edge_seq = EdgeSeq::new(new_seq as usize)?;
                }
            }
        }
        Ok(())
    }
}

/// Maps each of one edge kind's seqs to its post-compaction seq, or [`DEAD`].
///
/// Marking and numbering are two passes because a seq's new position depends on how many live
/// seqs precede it, which is only known once every half-edge has been seen.
fn live_seq_remap<S: Schema>(
    edge_storage: &EdgeStorage<S>,
    edge_kind: S::E,
    count: usize,
) -> Vec<u32> {
    let mut remap = vec![DEAD; count];

    for (node_kind, direction, kind) in S::edge_storage_slots_iter() {
        if kind != edge_kind {
            continue;
        }
        let slot_index = S::edge_storage_slot(node_kind, direction, kind).index();
        for edge_seq in edge_storage[slot_index].edges() {
            // A seq past the store cannot occur in a graph that passed `check_integrity`,
            // which every way of building one runs; ignoring it here keeps compaction
            // infallible rather than making it a second integrity check.
            if let Some(entry) = remap.get_mut(edge_seq.index()) {
                *entry = 0;
            }
        }
    }

    let mut next = 0u32;
    for entry in remap.iter_mut() {
        if *entry != DEAD {
            *entry = next;
            next += 1;
        }
    }
    remap
}

/// Rebuilds one store keeping only the live seqs, in their existing order.
///
/// Builds the new arrays before touching the store, so a failure leaves the graph as it was.
fn rebuild_store(
    store: &mut EdgePropertyStore,
    remap: &[u32],
    live_count: usize,
    is_multi: bool,
) -> Result<(), Error> {
    let old_values = store.values();
    let mut values = StorageArray::with_capacity(old_values.typ(), old_values.len());
    let mut offsets = Vec::new();

    if is_multi {
        let old_offsets = store.offsets();
        offsets.reserve(live_count + 1);
        offsets.push(Offset::zero());

        let mut cumulative = Offset::zero();
        for (seq, &new_seq) in remap.iter().enumerate() {
            if new_seq == DEAD {
                continue;
            }
            let (start, end) = store
                .get_offset(seq)
                .ok_or_else(|| Error::offsets_length_mismatch("", seq + 2, old_offsets.len()))?;
            for index in start.value()..end.value() {
                let value = old_values.get(index).ok_or_else(|| {
                    Error::property_index_out_of_bounds(index, end.value(), old_values.len())
                })?;
                values.try_push(&value)?;
            }
            cumulative = cumulative.checked_add_delta(end.checked_sub(start)?)?;
            offsets.push(cumulative);
        }
    } else {
        // `One` lets an `EdgeSeq` index `values` directly, so the seq is the position.
        for (seq, &new_seq) in remap.iter().enumerate() {
            if new_seq == DEAD {
                continue;
            }
            let value = old_values.get(seq).ok_or_else(|| {
                Error::property_index_out_of_bounds(seq, seq + 1, old_values.len())
            })?;
            values.try_push(&value)?;
        }
    }

    *store.values_mut() = values;
    *store.offsets_mut() = offsets;
    store.set_count(live_count);
    Ok(())
}
