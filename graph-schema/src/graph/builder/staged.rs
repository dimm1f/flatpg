use crate::{
    ItemIndex,
    error::Error,
    graph::Graph,
    node::{NodeId, RawNodeId},
    property::PropertyType,
    schema::Schema,
    storage::{
        EdgeStorageSlot, NodeMetaStorage, Offset, OffsetStorage, StorageArray, ranged_slice,
    },
};

// One property-storage slot fully rebuilt in place of existing nodes' data
// (from `Change::UpdateNodeProperty`). Commit is a plain assignment.
pub(super) struct PropertySlotReplace {
    pub(super) slot_index: usize,
    pub(super) values: StorageArray,
    pub(super) offsets: Vec<Offset>,
}

// One property-storage slot's append batch for brand-new nodes. Commit replays
// `try_append`/`extend` — proven infallible by the type/offset checks already
// performed while this record was built.
pub(super) struct PropertySlotAppend {
    pub(super) slot_index: usize,
    pub(super) values_batch: StorageArray,
    pub(super) offsets_tail: Vec<Offset>,
}

// One edge-storage slot fully rebuilt after edge removals (`Change::RemoveEdge`).
// Commit is a plain assignment.
pub(super) struct EdgeSlotReplace {
    pub(super) slot_index: usize,
    pub(super) neighbors: Vec<RawNodeId>,
    pub(super) values: StorageArray,
    pub(super) offsets: Vec<Offset>,
}

pub(super) struct EdgeSlotInsert {
    pub(super) slot_index: usize,
    pub(super) neighbors: Vec<RawNodeId>,
    pub(super) values: StorageArray,
    pub(super) batches: Vec<(usize, usize)>,
    pub(super) offsets: Vec<Offset>,
}

/// A validated set of changes, bound to the graph it was prepared against.
///
/// [`GraphDiff::prepare`](super::GraphDiff::prepare) resolves every change into the exact
/// writes it implies and hands them over in this form, holding the graph exclusively until
/// [`commit`](StagedDiff::commit) performs them. Dropping one instead leaves the graph as it
/// was.
#[must_use = "a staged diff leaves the graph unchanged until `commit` is called"]
pub struct StagedDiff<'g, S: Schema> {
    pub(super) graph: &'g mut Graph<S>,
    /// The string pool's length before staging, while the strings staging interned still have
    /// to be rolled back; `None` once [`StagedDiff::commit`] has taken them over.
    pub(super) strings_baseline: Option<usize>,
    pub(super) parts: StagedParts<S>,
}

pub(super) struct StagedParts<S: Schema> {
    pub(super) new_node_meta: NodeMetaStorage<S>,
    pub(super) node_remapper: Vec<NodeId<S>>,
    pub(super) node_tombstones: Vec<RawNodeId>,
    pub(super) property_replacements: Vec<PropertySlotReplace>,
    pub(super) edge_replacements: Vec<EdgeSlotReplace>,
    pub(super) property_appends: Vec<PropertySlotAppend>,
    pub(super) edge_inserts: Vec<EdgeSlotInsert>,
}

impl<S: Schema> Default for StagedParts<S> {
    fn default() -> Self {
        Self {
            new_node_meta: NodeMetaStorage::new(),
            node_remapper: Vec::new(),
            node_tombstones: Vec::new(),
            property_replacements: Vec::new(),
            edge_replacements: Vec::new(),
            property_appends: Vec::new(),
            edge_inserts: Vec::new(),
        }
    }
}

impl<S: Schema> Drop for StagedDiff<'_, S> {
    fn drop(&mut self) {
        if let Some(baseline) = self.strings_baseline {
            self.graph.strings.truncate(baseline);
        }
    }
}

impl<S: Schema> StagedDiff<'_, S> {
    /// Writes every staged change into the graph, returning the ids of the diff's new nodes.
    ///
    /// The ids come back in the order the nodes were added to the
    /// [`GraphDiff`](super::GraphDiff).
    ///
    /// # Panics
    ///
    /// Panics if the graph's flat storage does not hold the invariants
    /// [`CheckIntegrity::check_integrity`](crate::graph::integrity::CheckIntegrity::check_integrity)
    /// verifies, which every way of building a [`Graph<S>`] establishes.
    pub fn commit(mut self) -> Vec<NodeId<S>> {
        self.strings_baseline = None;
        let StagedParts {
            mut new_node_meta,
            node_remapper,
            node_tombstones,
            property_replacements,
            edge_replacements,
            property_appends,
            edge_inserts,
        } = std::mem::take(&mut self.parts);
        let graph = &mut *self.graph;

        for node_ref in &node_tombstones {
            if let Some(meta) = graph.node_meta_storage[node_ref.kind()].get_mut(node_ref.seq()) {
                meta.set_is_deleted(true);
            }
        }

        for replace in property_replacements {
            let slot = &mut graph.property_storage[replace.slot_index];
            *slot.values_mut() = replace.values;
            *slot.offsets_mut() = replace.offsets;
        }

        for replace in edge_replacements {
            let slot = &mut graph.edge_storage[replace.slot_index];
            *slot.neighbors_mut() = replace.neighbors;
            *slot.values_mut() = replace.values;
            *slot.offsets_mut() = replace.offsets;
        }

        // Safety: both graph.node_meta_storage and new_node_meta have number_of_node_kinds()
        // slots; kind.index() is always in-bounds; separate storages cannot alias.
        for kind in S::node_kinds() {
            let nodes = unsafe { graph.node_meta_storage.get_unchecked_mut(kind.index()) };
            let new_kind_nodes = unsafe { new_node_meta.get_unchecked_mut(kind.index()) };
            nodes.append(new_kind_nodes);
        }

        for append in property_appends {
            let slot = &mut graph.property_storage[append.slot_index];
            let mut values_batch = append.values_batch;
            // Panic: `try_append` only rejects a batch whose type differs from the slot's, and
            // `prepare` built this batch from this very slot's `typ()`; the slot cannot have
            // been retyped since, because this staged diff has held the graph exclusively.
            slot.values_mut()
                .try_append(&mut values_batch)
                .expect("staged property batch carries its slot's type");
            slot.offsets_mut().extend(append.offsets_tail);
        }

        for insert in edge_inserts {
            let slot = &mut graph.edge_storage[insert.slot_index];
            if !insert.batches.is_empty() {
                // Panic: the merge only fails on a batch position behind the neighbors already
                // copied or past the end of the slot's arrays, which takes offsets that are not
                // monotonic or overrun their arrays. Every `Graph<S>` rules that out —
                // `Graph::new` starts empty, `TryFrom<RawGraph<S>>` runs `check_integrity`, and
                // this method is the only other writer — and `prepare` derived these positions
                // from the offsets left by the edge replacements applied above.
                merge_edge_batches(slot, &insert.batches, insert.neighbors, insert.values)
                    .expect("staged edge batches fit the slot they were measured against");
            }
            *slot.offsets_mut() = insert.offsets;
        }

        node_remapper
    }
}

// Rebuilds `slot`'s arrays in one pass, alternating runs copied from the old ones with
// runs taken from the batch arrays. Splicing each batch in separately would cost O(tail)
// apiece, so a slot receiving k batches would move O(k * slot size) elements — the same
// quadratic `prepare_property_updates` and `prepare_edge_removals` are written to avoid.
fn merge_edge_batches(
    slot: &mut EdgeStorageSlot,
    batches: &[(usize, usize)],
    batch_neighbors: Vec<RawNodeId>,
    batch_values: StorageArray,
) -> Result<(), Error> {
    let inserted = batch_neighbors.len();

    let old_neighbors = std::mem::take(slot.neighbors_mut());
    let values_typ = slot.values().typ();
    let has_values = values_typ != PropertyType::None;
    let old_values = std::mem::take(slot.values_mut());

    if old_neighbors.is_empty() {
        *slot.neighbors_mut() = batch_neighbors;
        if has_values {
            *slot.values_mut() = batch_values;
        }
        return Ok(());
    }

    let mut neighbors = Vec::with_capacity(old_neighbors.len() + inserted);
    let mut values = StorageArray::with_capacity(values_typ, old_neighbors.len() + inserted);

    let mut copied = 0usize;
    let mut taken = 0usize;
    for &(place, count) in batches {
        // Zero when this batch abuts the previous one.
        let take = place
            .checked_sub(neighbors.len())
            .ok_or_else(Error::offset_underflow)?;
        if take > 0 {
            neighbors.extend_from_slice(ranged_slice(&old_neighbors, copied..copied + take)?);
            if has_values {
                values.try_extend_from_range(&old_values, copied..copied + take)?;
            }
            copied += take;
        }

        neighbors.extend_from_slice(ranged_slice(&batch_neighbors, taken..taken + count)?);
        if has_values {
            values.try_extend_from_range(&batch_values, taken..taken + count)?;
        }
        taken += count;
    }

    neighbors.extend_from_slice(ranged_slice(&old_neighbors, copied..old_neighbors.len())?);
    if has_values {
        values.try_extend_from_range(&old_values, copied..old_neighbors.len())?;
        *slot.values_mut() = values;
    }
    *slot.neighbors_mut() = neighbors;
    Ok(())
}
