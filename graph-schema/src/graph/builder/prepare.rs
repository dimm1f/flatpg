use crate::{
    EdgeDirectionKind, ItemAsStr, ItemIndex,
    edge::Direction,
    error::Error,
    graph::{Graph, node_is_deleted},
    node::{NodeId, NodeMeta, RawNodeId},
    property::PropertyValue,
    schema::{EdgeKind, Schema},
    storage::{EdgeStorage, NodeMetaStorage, Offset, OffsetStorage, StoredProperty},
};

use super::convert::to_stored_property;
use super::slots::{
    SlotBuckets, prepare_edge_removals, prepare_new_edges, prepare_new_node_properties,
    prepare_property_updates,
};
use super::staged::StagedDiff;
use super::{Change, GraphDiff, HalfEdge, NewEdge, NewOrExistingNode, QuantifiedProperty};

impl<S: Schema> GraphDiff<S> {
    pub fn prepare(&self, graph: &mut Graph<S>) -> Result<StagedDiff<S>, Error> {
        let (mut node_tombstones, slot_property_updates, slot_edge_removals) =
            classify_changes(&self.changes, graph)?;

        let property_replacements = prepare_property_updates(
            &graph.property_storage,
            &mut graph.strings,
            slot_property_updates,
        )?;
        let edge_replacements = prepare_edge_removals(&graph.edge_storage, slot_edge_removals)?;

        // Offset baselines for slots also touched by a new-node/new-edge below, in the
        // same diff: without these, new-node/new-edge processing would compute starting
        // offsets from the graph's pre-diff state instead of the post-replacement state
        // `StagedDiff::commit` will have already written by the time it gets to appends/inserts.
        let mut property_last_offset: Vec<Option<Offset>> = vec![None; S::property_storage_size()];
        for r in &property_replacements {
            property_last_offset[r.slot_index] = r.offsets.last().copied();
        }
        let mut edge_offsets_baseline: Vec<Option<&Vec<Offset>>> =
            vec![None; S::edge_storage_size()];
        for r in &edge_replacements {
            edge_offsets_baseline[r.slot_index] = Some(&r.offsets);
        }

        // Note: `node_remapper` must contain nodes with actual NodeSeq.
        // Therefore, the max seq per kind must be obtained from graph before mapping.
        let mut node_remapper: Vec<RawNodeId> = Vec::with_capacity(self.new_nodes.len());
        let graph_nodes_max_seq = S::node_kinds()
            .iter()
            .map(|k| graph.node_count_by_kind_with_deleted(*k))
            .collect::<Vec<_>>();

        let new_nodes_count = self.new_nodes.iter().fold(
            vec![0usize; S::number_of_node_kinds()],
            |mut acc, new_node| {
                acc[new_node.kind().index()] += 1;
                acc
            },
        );

        let mut new_nodes: NodeMetaStorage<S> = NodeMetaStorage::new();

        let mut seq_counters = vec![0usize; S::number_of_node_kinds()];

        let mut slot_property: SlotBuckets<Option<&Vec<PropertyValue>>> =
            SlotBuckets::new(S::property_storage_size());

        for node in self.new_nodes.iter() {
            // Safety: seq_counters has number_of_node_kinds() elements; node.kind().index() is always in-bounds.
            let current_seq = unsafe { seq_counters.get_unchecked_mut(node.kind().index()) };

            let local_index = *current_seq;
            let seq = local_index + graph_nodes_max_seq[node.kind().index()];
            *current_seq += 1;

            // Safety: new_nodes has number_of_node_kinds() slots; node.kind().index() is always in-bounds.
            let nodes_storage = unsafe { new_nodes.get_unchecked_mut(node.kind().index()) };
            nodes_storage.push(NodeMeta::default());

            node_remapper.push(RawNodeId::new(node.kind().index(), seq));

            for (prop_kind, new_values) in node.properties() {
                let slot_index = S::property_storage_slot(node.kind(), *prop_kind).index();
                let nodes_count = new_nodes_count[node.kind().index()];
                slot_property.bucket(slot_index, nodes_count)[local_index] = Some(new_values);
            }
        }

        let property_appends = prepare_new_node_properties(
            &graph.property_storage,
            &mut graph.strings,
            &graph_nodes_max_seq,
            &new_nodes_count,
            slot_property,
            &property_last_offset,
        )?;

        // Only consulted by the new-edge loop below, so a diff that adds no edges never
        // pays for it. Sorting `node_tombstones` in place is free: `commit` only iterates
        // it to set tombstone flags, where order is irrelevant.
        let staged_removed: &[RawNodeId] = if self.new_edges.is_empty() {
            &[]
        } else {
            node_tombstones.sort_unstable();
            &node_tombstones
        };

        let resolve_node_ref = |node: &NewOrExistingNode| -> Option<RawNodeId> {
            match node {
                NewOrExistingNode::New(id) => node_remapper.get(*id).copied(),
                NewOrExistingNode::Existing(node_ref) => Some(*node_ref),
            }
        };

        let mut slot_edge_halves: SlotBuckets<Vec<HalfEdge<S>>> =
            SlotBuckets::new(S::edge_storage_size());

        for new_edge in &self.new_edges {
            let property = new_edge
                .property
                .as_ref()
                .map(|prop| to_stored_property(prop, &mut graph.strings));
            let Some(halves) = edge_to_halves(new_edge, resolve_node_ref, property) else {
                continue;
            };
            // Both halves must survive together: an edge with either endpoint deleted is
            // dropped entirely, not partially. A brand-new node (seq >= its kind's
            // pre-diff node count) is never "deleted" — it doesn't exist in
            // `graph.node_meta_storage` yet at all (node metadata append is deferred to
            // `StagedDiff::commit`), so node_is_deleted would otherwise misread it as
            // out-of-range and drop every edge touching it. Only existing nodes go
            // through the deleted-in-graph / staged-for-removal checks.
            if halves.iter().any(|h| {
                let seq = h.node.seq();
                let kind_index = h.node.kind().index();
                seq < graph_nodes_max_seq[kind_index]
                    && (node_is_deleted::<S>(&graph.node_meta_storage, h.node)
                        || staged_removed
                            .binary_search(&RawNodeId::from(&h.node))
                            .is_ok())
            }) {
                continue;
            }
            for half in halves {
                let slot_index =
                    S::edge_storage_slot(half.node.kind(), half.direction, half.edge_kind).index();
                let node_count = graph_nodes_max_seq[half.node.kind().index()]
                    + new_nodes_count[half.node.kind().index()];
                slot_edge_halves.bucket(slot_index, node_count)[half.node.seq()].push(half);
            }
        }

        let edge_inserts = prepare_new_edges(
            &graph.edge_storage,
            &graph_nodes_max_seq,
            &new_nodes_count,
            slot_edge_halves,
            &edge_offsets_baseline,
        )?;

        let node_remapper = node_remapper
            .into_iter()
            .map(NodeId::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(StagedDiff {
            new_node_meta: new_nodes,
            node_remapper,
            node_tombstones,
            property_replacements,
            edge_replacements,
            property_appends,
            edge_inserts,
        })
    }
}

type ClassifiedChanges<'a> = (
    Vec<RawNodeId>,
    SlotBuckets<Option<&'a QuantifiedProperty>>,
    SlotBuckets<Vec<usize>>,
);

// Buckets every pending `Change` by kind: node tombstones, per-slot property-update
// pointers, and per-slot local edge-removal seqs. `node_tombstones` doubles as the
// staged-for-removal set the new-edge loop consults: a node is pushed here on exactly
// the in-range condition that guards that lookup, so it needs no separate collection.
fn classify_changes<'a, S: Schema>(
    changes: &'a [Change<S>],
    graph: &Graph<S>,
) -> Result<ClassifiedChanges<'a>, Error> {
    let mut node_tombstones = Vec::new();
    let mut slot_property_updates: SlotBuckets<Option<&QuantifiedProperty>> =
        SlotBuckets::new(S::property_storage_size());
    let mut slot_edge_removals: SlotBuckets<Vec<usize>> = SlotBuckets::new(S::edge_storage_size());

    for change in changes {
        match change {
            Change::RemoveNode(node_ref) => {
                let node: NodeId<S> = (*node_ref).try_into()?;
                if graph.node_meta_storage[node.kind().index()]
                    .get(node_ref.seq())
                    .is_some()
                {
                    node_tombstones.push(*node_ref);
                }
            }
            Change::UpdateNodeProperty(node_ref, property_kind, quantified_property) => {
                let node: NodeId<S> = (*node_ref).try_into()?;
                let slot_index = S::property_storage_slot(node.kind(), *property_kind).index();
                let node_count = graph.node_meta_storage[node.kind().index()].len();

                let Some(slot) = slot_property_updates
                    .bucket(slot_index, node_count)
                    .get_mut(node_ref.seq())
                else {
                    return Err(Error::node_offset_not_found(node_ref.seq()));
                };
                *slot = Some(quantified_property);
            }
            Change::RemoveEdge(edge) => {
                let src = edge.src_node_id();
                let dst = edge.dst();
                let edge_kind = S::resolve_edge_kind(edge.handle())?;
                let primary_seq = edge.handle().seq();

                let (primary, primary_dir, secondary, secondary_dir) =
                    S::resolve_edge_direction(edge.handle())?.orient_edge(src, dst);

                // Record the primary half first: this is what detects the same edge
                // being queued for removal twice in one diff, which collapses into a
                // single removal instead of being processed twice. This needs to happen
                // before the secondary-half scan below, or that scan would instead fail
                // once its position was already excluded by the first occurrence.
                let primary_kind = S::resolve_node_kind(primary)?;
                let primary_slot_index =
                    S::edge_storage_slot(primary_kind, primary_dir, edge_kind).index();
                let primary_count = graph.node_meta_storage[primary_kind.index()].len();

                let bucket = &mut slot_edge_removals.bucket(primary_slot_index, primary_count)
                    [primary.seq()];

                if bucket.contains(&primary_seq) {
                    continue;
                }

                bucket.push(primary_seq);

                let secondary_kind = S::resolve_node_kind(secondary)?;
                let secondary_slot_index =
                    S::edge_storage_slot(secondary_kind, secondary_dir, edge_kind).index();
                let secondary_count = graph.node_meta_storage[secondary_kind.index()].len();

                let already_claimed = slot_edge_removals
                    .get(secondary_slot_index)
                    .and_then(|bucket| bucket.get(secondary.seq()))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);

                let secondary_seq = find_reverse_edge_seq(
                    &graph.edge_storage,
                    secondary,
                    secondary_slot_index,
                    secondary_dir,
                    edge_kind,
                    primary,
                    already_claimed,
                )?;

                slot_edge_removals.bucket(secondary_slot_index, secondary_count)[secondary.seq()]
                    .push(secondary_seq);
            }
        }
    }

    Ok((node_tombstones, slot_property_updates, slot_edge_removals))
}

// Searches `node`'s half-edges in `slot_index` for `target`, skipping any local seq already
// in `excluded`. Takes `slot_index` precomputed by the caller (who already needed it to
// resolve `node`'s kind for other purposes) rather than re-deriving it from `node` here.
fn find_reverse_edge_seq<S>(
    edge_storage: &EdgeStorage<S>,
    node: RawNodeId,
    slot_index: usize,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    target: RawNodeId,
    excluded: &[usize],
) -> Result<usize, Error>
where
    S: Schema,
{
    let slot = &edge_storage[slot_index];

    let Some((start, end)) = slot.get_offset(node.seq()) else {
        return Err(Error::node_offset_not_found(node.seq()));
    };

    slot.get_neighbors(start, end)
        .enumerate()
        .find(|(local_seq, neighbor)| *neighbor == target && !excluded.contains(local_seq))
        .map(|(local_seq, _)| local_seq)
        .ok_or_else(|| match (node.try_into(), target.try_into()) {
            (Ok::<NodeId<S>, _>(node), Ok::<NodeId<S>, _>(target)) => {
                Error::reverse_edge_not_found(
                    target.to_string(),
                    node.to_string(),
                    direction.as_str().to_owned(),
                    edge_kind.as_str().to_owned(),
                )
            }
            (Err(e), _) | (_, Err(e)) => e,
        })
}

fn edge_to_halves<F, S>(
    new_edge: &NewEdge<S>,
    node_resolver: F,
    property: Option<StoredProperty>,
) -> Option<[HalfEdge<S>; 2]>
where
    F: Fn(&NewOrExistingNode) -> Option<RawNodeId>,
    S: Schema,
{
    let src_node = node_resolver(&new_edge.src)?;
    let dst_node = node_resolver(&new_edge.dst)?;

    let src_half = HalfEdge {
        edge_kind: new_edge.kind,
        node: src_node.try_into().ok()?,
        neighbor: dst_node.try_into().ok()?,
        direction: Direction::src_half(),
        property: property.clone(),
    };

    let dst_half = HalfEdge {
        edge_kind: new_edge.kind,
        node: dst_node.try_into().ok()?,
        neighbor: src_node.try_into().ok()?,
        direction: Direction::dst_half(),
        property,
    };

    Some([src_half, dst_half])
}
