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

// One edge-storage slot's insertion batches for new edges, one entry per touched
// node-seq, precomputed in ascending `place` order. Commit replays `splice`/
// `try_splice` — proven infallible by the checks already performed while this
// record was built.
pub(super) struct EdgeSlotInsert {
    pub(super) slot_index: usize,
    pub(super) splices: Vec<(usize, Vec<RawNodeId>, Option<StorageArray>)>,
    pub(super) offsets: Vec<Offset>,
}

pub struct StagedDiff<S: Schema> {
    pub(super) new_node_meta: NodeMetaStorage<S>,
    pub(super) node_remapper: Vec<NodeId<S>>,
    pub(super) node_tombstones: Vec<RawNodeId>,
    pub(super) property_replacements: Vec<PropertySlotReplace>,
    pub(super) edge_replacements: Vec<EdgeSlotReplace>,
    pub(super) property_appends: Vec<PropertySlotAppend>,
    pub(super) edge_inserts: Vec<EdgeSlotInsert>,
}

impl<S: Schema> StagedDiff<S> {
    // Writes every already-validated piece of `self` into `graph`. Every check that
    // could have failed was already performed by `prepare` — see the `Result` note on
    // the type doc above. `graph` must be the same graph, in the same state, that
    // `prepare` built `self` from — see the `StagedDiff` type doc for what breaks if
    // it isn't.
    pub fn commit(self, graph: &mut Graph<S>) -> Result<Vec<NodeId<S>>, Error> {
        let StagedDiff {
            mut new_node_meta,
            node_remapper,
            node_tombstones,
            property_replacements,
            edge_replacements,
            property_appends,
            edge_inserts,
        } = self;

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
            slot.values_mut().try_append(&mut values_batch)?;
            slot.offsets_mut().extend(append.offsets_tail);
        }

        for insert in edge_inserts {
            let slot = &mut graph.edge_storage[insert.slot_index];
            if !insert.splices.is_empty() {
                merge_splices(slot, insert.splices)?;
            }
            *slot.offsets_mut() = insert.offsets;
        }

        Ok(node_remapper)
    }
}

fn merge_splices(
    slot: &mut EdgeStorageSlot,
    splices: Vec<(usize, Vec<RawNodeId>, Option<StorageArray>)>,
) -> Result<(), Error> {
    let inserted: usize = splices.iter().map(|(_, batch, _)| batch.len()).sum();

    let old_neighbors = std::mem::take(slot.neighbors_mut());
    let mut neighbors = Vec::with_capacity(old_neighbors.len() + inserted);

    let values_typ = slot.values().typ();
    let has_values = values_typ != PropertyType::None;
    let old_values = std::mem::take(slot.values_mut());
    let mut values = StorageArray::with_capacity(values_typ, old_values.len() + inserted);

    let mut copied = 0usize;
    for (place, batch, batch_values) in splices {
        // Zero when this batch abuts the previous one, which is every batch when the
        // slot starts empty (a bulk load into a fresh graph): all `place`s are appends.
        let take = place - neighbors.len();
        if take > 0 {
            neighbors.extend_from_slice(ranged_slice(&old_neighbors, copied..copied + take)?);
            if has_values {
                values.try_extend_from_range(&old_values, copied..copied + take)?;
            }
            copied += take;
        }

        neighbors.extend(batch);
        if let Some(mut batch_values) = batch_values {
            values.try_append(&mut batch_values)?;
        }
    }

    neighbors.extend_from_slice(ranged_slice(&old_neighbors, copied..old_neighbors.len())?);
    if has_values {
        values.try_extend_from_range(&old_values, copied..old_values.len())?;
        *slot.values_mut() = values;
    }
    *slot.neighbors_mut() = neighbors;
    Ok(())
}
