use crate::{
    ItemIndex,
    graph::Graph,
    node::{NodeId, RawNodeId},
    schema::Schema,
    storage::{NodeMetaStorage, Offset, OffsetStorage, StorageArray},
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

// Fully-validated, ready-to-write result of `GraphDiff::prepare`. Nothing in
// `StagedDiff::commit` can fail: every fallible check (`try_push`/`try_append`/
// `try_splice`, `checked_add_delta`/`checked_sub`/`checked_sub_delta`,
// `NodeId::try_from`) has already been resolved while building this value, against
// the graph's pre-mutation state, so `commit` only ever replays already-validated
// data.
//
// That guarantee holds only for the exact `Graph<S>` and state `prepare` built this
// value against. Calling `commit` on a different `Graph<S>`, or on the same one after
// something else has already mutated it, is a logic error the type system cannot
// catch: the offsets and splice positions baked into this value would no longer match
// the graph's actual layout, and `commit` would replay them anyway — panicking if a
// position falls outside the current storage, or silently writing misaligned data
// otherwise. Always commit a `StagedDiff` immediately against the same graph state
// `prepare` just validated it against, as `GraphDiff::apply` does.
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
    // Writes every already-validated piece of `self` into `graph`. Cannot fail: every
    // check that could have failed was already performed by `prepare`. `graph` must
    // be the same graph, in the same state, that `prepare` built `self` from — see
    // the `StagedDiff` type doc for what breaks if it isn't.
    pub fn commit(self, graph: &mut Graph<S>) -> Vec<NodeId<S>> {
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
            slot.values_mut()
                .try_append(&mut values_batch)
                .expect("type validated in prepare");
            slot.offsets_mut().extend(append.offsets_tail);
        }

        for insert in edge_inserts {
            let slot = &mut graph.edge_storage[insert.slot_index];
            for (place, neighbors, values) in insert.splices {
                slot.neighbors_mut().splice(place..place, neighbors);
                if let Some(values) = values {
                    slot.values_mut()
                        .try_splice(place, values)
                        .expect("type/position validated in prepare");
                }
            }
            *slot.offsets_mut() = insert.offsets;
        }

        node_remapper
    }
}
