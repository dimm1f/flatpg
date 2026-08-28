use crate::{
    EdgeDirectionKind, ItemAsStr, ItemIndex,
    edge::{Direction, EdgeId, RawEdgeId},
    error::Error,
    graph::{Graph, GraphView, node_is_deleted},
    node::{NewNode, NodeId, NodeMeta, RawNodeId},
    property::{PropertyType, PropertyValue},
    schema::{EdgeKind, PropKind, Schema},
    storage::{NodeMetaStorage, Offset, OffsetStorage, StorageArray, StoredProperty},
    strings_pool::StringsPool,
};

type NewEdgeId = usize;

struct HalfEdge<S: Schema> {
    node: NodeId<S>,
    neighbor: NodeId<S>,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    property: Option<StoredProperty>,
}

struct NewEdge<S: Schema> {
    src: NewOrExistingNode,
    dst: NewOrExistingNode,
    kind: EdgeKind<S>,
    property: Option<PropertyValue>,
}

struct SlotBuckets<T>(Vec<Option<Vec<T>>>);

impl<T> SlotBuckets<T> {
    fn new(slot_count: usize) -> Self {
        Self((0..slot_count).map(|_| None).collect())
    }

    fn get(&self, slot_index: usize) -> Option<&Vec<T>> {
        self.0[slot_index].as_ref()
    }

    fn take(&mut self, slot_index: usize) -> Option<Vec<T>> {
        self.0[slot_index].take()
    }
}

impl<T: Default> SlotBuckets<T> {
    fn bucket(&mut self, slot_index: usize, len: usize) -> &mut Vec<T> {
        self.0[slot_index].get_or_insert_with(|| (0..len).map(|_| T::default()).collect())
    }
}

type ChangeId = usize;
enum Change<S: Schema> {
    RemoveNode(RawNodeId),
    UpdateNodeProperty(RawNodeId, PropKind<S>, QuantifiedProperty),
    RemoveEdge(RawEdgeId),
}

#[derive(Debug, Clone)]
pub enum QuantifiedProperty {
    One(PropertyValue),
    Multi(Vec<PropertyValue>),
}

impl From<PropertyValue> for QuantifiedProperty {
    fn from(value: PropertyValue) -> Self {
        Self::One(value)
    }
}

impl From<&PropertyValue> for QuantifiedProperty {
    fn from(value: &PropertyValue) -> Self {
        Self::One(value.clone())
    }
}

impl From<Vec<PropertyValue>> for QuantifiedProperty {
    fn from(value: Vec<PropertyValue>) -> Self {
        Self::Multi(value)
    }
}

impl From<&Vec<PropertyValue>> for QuantifiedProperty {
    fn from(value: &Vec<PropertyValue>) -> Self {
        Self::Multi(value.clone())
    }
}

impl From<&[PropertyValue]> for QuantifiedProperty {
    fn from(value: &[PropertyValue]) -> Self {
        Self::Multi(value.to_vec())
    }
}

type NewNodeId = usize;

pub enum NewOrExistingNode {
    New(NewNodeId),
    Existing(RawNodeId),
}

impl From<NewNodeId> for NewOrExistingNode {
    fn from(value: NewNodeId) -> Self {
        Self::New(value)
    }
}

impl From<RawNodeId> for NewOrExistingNode {
    fn from(value: RawNodeId) -> Self {
        Self::Existing(value)
    }
}

impl<S: Schema> From<NodeId<S>> for NewOrExistingNode {
    fn from(value: NodeId<S>) -> Self {
        Self::Existing(RawNodeId::from(&value))
    }
}

#[derive(Default)]
pub struct GraphDiff<S: Schema> {
    new_nodes: Vec<NewNode<S>>,
    new_edges: Vec<NewEdge<S>>,
    changes: Vec<Change<S>>,
}

impl<S: Schema> GraphDiff<S> {
    pub fn add_node(&mut self, node: NewNode<S>) -> NewNodeId {
        self.new_nodes.push(node);
        self.new_nodes.len() - 1
    }

    #[inline]
    pub fn add_edge<T, U>(
        &mut self,
        src: T,
        dst: U,
        kind: EdgeKind<S>,
        property: Option<PropertyValue>,
    ) -> NewEdgeId
    where
        T: Into<NewOrExistingNode>,
        U: Into<NewOrExistingNode>,
    {
        self.add_edge_inner(src.into(), dst.into(), kind, property)
    }

    fn add_edge_inner(
        &mut self,
        src: NewOrExistingNode,
        dst: NewOrExistingNode,
        kind: EdgeKind<S>,
        property: Option<PropertyValue>,
    ) -> NewEdgeId {
        let edge = NewEdge {
            src,
            dst,
            kind,
            property,
        };

        self.new_edges.push(edge);
        self.new_edges.len() - 1
    }

    pub fn remove_node<T: Into<RawNodeId>>(&mut self, node_ref: T) -> ChangeId {
        self.changes.push(Change::RemoveNode(node_ref.into()));
        self.changes.len() - 1
    }

    pub fn remove_edge<T: Into<EdgeId<S>>>(&mut self, edge: T) -> ChangeId {
        let edge: EdgeId<S> = edge.into();
        self.changes.push(Change::RemoveEdge((&edge).into()));
        self.changes.len() - 1
    }

    pub fn update_node_property<T: Into<RawNodeId>, P: Into<QuantifiedProperty>>(
        &mut self,
        node_ref: T,
        property_kind: PropKind<S>,
        value: P,
    ) -> ChangeId {
        self.changes.push(Change::UpdateNodeProperty(
            node_ref.into(),
            property_kind,
            value.into(),
        ));
        self.changes.len() - 1
    }

    // NOTE: Be careful when you apply several diffs that were built from the same graph, one
    // after another. Ids from an earlier diff can become wrong once an earlier `apply` call
    // changes the graph, and this does not always cause an error:
    // - `remove_edge` finds a half-edge by its position in the node's own edge list. When one
    //   diff removes an edge, every later edge on that node moves one position down. If another
    //   diff still holds an edge id from before that removal, it may now point to a different
    //   edge on the same node and remove it silently, with no error.
    // - `remove_node` only marks a node as deleted, so node ids stay valid across diffs. But
    //   `update_node_property` does not check whether the node was already deleted by an
    //   earlier diff, so a later diff can still write a property onto a deleted node.
    // To stay safe, apply one diff, then build the next diff from the graph `apply` just
    // returned, instead of reusing ids from before that call.
    pub fn apply(self, graph: impl GraphView<S>) -> Result<(Graph<S>, Vec<NodeId<S>>), Error> {
        let mut graph = graph.into_graph();
        self.apply_changes(&mut graph)?;

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

        // WARN: May break the graph. This appends new node metadata unconditionally. If the
        // property/edge population below fails partway, these nodes stay in `graph` without
        // fully populated storage, since none of this is rolled back on error.
        for kind in S::node_kinds() {
            // Safety: both graph.node_meta_storage and new_nodes have number_of_node_kinds()
            // slots; kind.index() is in-bounds; separate storages cannot alias.
            let nodes = unsafe { graph.node_meta_storage.get_unchecked_mut(kind.index()) };
            let new_kind_nodes = unsafe { new_nodes.get_unchecked_mut(kind.index()) };
            nodes.append(new_kind_nodes);
        }

        // WARN: May break the graph. Properties and edges are inserted directly into the graph so
        // any issues at this stage can corrupt the graph. This could be fixed by implementing
        // transactions (staging these mutations and only committing them once the whole diff
        // has applied successfully).
        for (node_kind, property_kind) in S::property_storage_slots_iter() {
            let nodes_count = new_nodes_count[node_kind.index()];
            if nodes_count == 0 {
                continue;
            }

            let slot_index = S::property_storage_slot(node_kind, property_kind).index();
            let slot = &mut graph.property_storage[slot_index];

            let Some(seq_property) = slot_property.get(slot_index) else {
                if !slot.offsets().is_empty() {
                    let new_len = slot.offsets().len() + nodes_count;
                    let last = slot.offsets().last().copied().unwrap_or_else(Offset::zero);
                    slot.offsets_mut().resize(new_len, last);
                }
                continue;
            };

            if slot.offsets().is_empty() {
                let existing = graph_nodes_max_seq[node_kind.index()];
                slot.offsets_mut().resize(existing + 1, Offset::zero());
            }

            let mut cumulative = slot.offsets().last().copied().unwrap_or_else(Offset::zero);
            for local_index in 0..nodes_count {
                if let Some(props) = seq_property.get(local_index).copied().flatten() {
                    let mut batch =
                        stored_property_batch(props, slot.values().typ(), &mut graph.strings)?;
                    cumulative = cumulative.checked_add_delta(props.len())?;
                    slot.values_mut().try_append(&mut batch)?;
                }
                slot.offsets_mut().push(cumulative);
            }
        }

        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let nodes_count = new_nodes_count[node_kind.index()];
            if nodes_count == 0 {
                continue;
            }
            let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind).index();
            let slot = &mut graph.edge_storage[slot_index];
            if slot.offsets().is_empty() {
                continue;
            }
            let new_len = slot.offsets().len() + nodes_count;
            let last = slot.offsets().last().copied().unwrap_or_else(Offset::zero);
            slot.offsets_mut().resize(new_len, last);
        }

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
            // dropped entirely, not partially. This also guarantees every half's seq()
            // below is in-bounds (node_is_deleted treats an out-of-range seq as deleted
            // too), which is what makes the direct bucket indexing below safe.
            if halves
                .iter()
                .any(|h| node_is_deleted::<S>(&graph.node_meta_storage, h.node))
            {
                continue;
            }
            for half in halves {
                let slot_index =
                    S::edge_storage_slot(half.node.kind(), half.direction, half.edge_kind).index();
                let node_count = graph.node_meta_storage[half.node.kind().index()].len();
                slot_edge_halves.bucket(slot_index, node_count)[half.node.seq()].push(half);
            }
        }

        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind).index();
            let Some(seq_halves) = slot_edge_halves.take(slot_index) else {
                continue;
            };
            let slot = &mut graph.edge_storage[slot_index];

            let offsets_len = graph.node_meta_storage[node_kind.index()].len() + 1;
            if slot.offsets().is_empty() {
                *slot.offsets_mut() = vec![Offset::zero(); offsets_len];
            } else if slot.offsets().len() != offsets_len {
                let last = slot.offsets().last().cloned().unwrap_or_else(Offset::zero);
                slot.offsets_mut().resize(offsets_len, last);
            }

            let mut delta = 0;

            for end in 1..slot.offsets().len() {
                let start = end - 1;

                let halves = &seq_halves[start];
                if !halves.is_empty() {
                    let new_neighbors = halves.iter().map(|h| RawNodeId::from(&h.neighbor));

                    let place = slot.offsets()[end].value() + delta;
                    slot.neighbors_mut().splice(place..place, new_neighbors);
                    delta += halves.len();

                    let property_type = slot.values().typ();
                    if property_type != PropertyType::None {
                        let mut batch = StorageArray::with_capacity(property_type, halves.len());
                        for half in halves.iter() {
                            if let Some(prop) = &half.property {
                                batch.try_push(prop)?;
                            }
                        }
                        slot.values_mut().try_splice(place, batch)?;
                    }
                }

                slot.offsets_mut()[end] = slot.offsets()[end].checked_add_delta(delta)?;
            }
        }

        let node_remapper = node_remapper
            .into_iter()
            .map(NodeId::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok((graph, node_remapper))
    }

    fn apply_changes(&self, graph: &mut Graph<S>) -> Result<(), Error> {
        let mut slot_property_updates: SlotBuckets<Option<&QuantifiedProperty>> =
            SlotBuckets::new(S::property_storage_size());
        let mut slot_edge_removals: SlotBuckets<Vec<usize>> =
            SlotBuckets::new(S::edge_storage_size());

        for change in &self.changes {
            match change {
                Change::RemoveNode(node_ref) => {
                    let node: NodeId<S> = (*node_ref).try_into()?;
                    if let Some(seq) =
                        graph.node_meta_storage[node.kind().index()].get_mut(node_ref.seq())
                    {
                        // WARN: May break the graph. This tombstone is written immediately and
                        // isn't rolled back if a later change in this same diff fails below.
                        seq.set_is_deleted(true);
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
                        graph,
                        secondary,
                        secondary_slot_index,
                        secondary_dir,
                        edge_kind,
                        primary,
                        already_claimed,
                    )?;

                    slot_edge_removals.bucket(secondary_slot_index, secondary_count)
                        [secondary.seq()]
                    .push(secondary_seq);
                }
            }
        }

        apply_property_updates(graph, slot_property_updates)?;
        apply_edge_removals(graph, slot_edge_removals)?;
        Ok(())
    }
}

fn apply_property_updates<S: Schema>(
    graph: &mut Graph<S>,
    mut pending: SlotBuckets<Option<&QuantifiedProperty>>,
) -> Result<(), Error> {
    for (node_kind, property_kind) in S::property_storage_slots_iter() {
        let slot_index = S::property_storage_slot(node_kind, property_kind).index();
        let Some(seq_updates) = pending.take(slot_index) else {
            continue;
        };
        let slot = &mut graph.property_storage[slot_index];

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

            let count = if let Some(quantified_property) = seq_updates[start] {
                let new_node_values: &[PropertyValue] = match quantified_property {
                    QuantifiedProperty::One(p) => std::slice::from_ref(p),
                    QuantifiedProperty::Multi(ps) => ps.as_slice(),
                };
                let mut batch =
                    stored_property_batch(new_node_values, property_type, &mut graph.strings)?;
                let count = batch.len();
                new_values.try_append(&mut batch)?;
                count
            } else {
                for prop in slot
                    .values()
                    .iter_range(orig_start.value()..orig_end.value())
                {
                    new_values.try_push(&prop)?;
                }
                orig_end.checked_sub(orig_start)?
            };

            cumulative = cumulative.checked_add_delta(count)?;
            new_offsets.push(cumulative);
        }

        // WARN: May break the graph. This commits the rebuilt slot immediately. If a later slot in
        // this loop fails, earlier committed slots stay mutated while later ones never apply.
        *slot.values_mut() = new_values;
        *slot.offsets_mut() = new_offsets;
    }
    Ok(())
}

fn apply_edge_removals<S: Schema>(
    graph: &mut Graph<S>,
    mut pending: SlotBuckets<Vec<usize>>,
) -> Result<(), Error> {
    for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
        let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind).index();
        let Some(seq_removals) = pending.take(slot_index) else {
            continue;
        };
        let slot = &mut graph.edge_storage[slot_index];
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
        // WARN: May break the graph. Neighbors/values are committed immediately, and the offsets
        // shift below can still fail partway, or a later slot in this loop can fail entirely,
        // leaving this slot's storage mutated with no rollback.
        *slot.neighbors_mut() = new_neighbors;
        *slot.values_mut() = new_values;

        // Per-node removal counts, not `absolute_removals`, drive the shift here: each
        // node's own count from `seq_removals` is already exactly how much every offset
        // from that node onward must decrease by, with no need to re-derive it from the
        // sorted absolute-position array the filter pass above used.
        let mut removed_so_far = 0usize;
        for end in 1..slot.offsets().len() {
            removed_so_far += seq_removals[end - 1].len();
            slot.offsets_mut()[end] = slot.offsets()[end].checked_sub_delta(removed_so_far)?;
        }
    }
    Ok(())
}

fn find_reverse_edge_seq<S>(
    graph: &Graph<S>,
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
    let slot = &graph.edge_storage[slot_index];

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

fn stored_property_batch(
    values: &[PropertyValue],
    typ: PropertyType,
    strings: &mut StringsPool,
) -> Result<StorageArray, Error> {
    let mut batch = StorageArray::with_capacity(typ, values.len());
    for prop in values {
        let prop = to_stored_property(prop, strings);
        batch.try_push(&prop)?;
    }
    Ok(batch)
}

fn to_stored_property(prop: &PropertyValue, strings: &mut StringsPool) -> StoredProperty {
    match prop {
        PropertyValue::Bool(v) => StoredProperty::Bool(*v),
        PropertyValue::Byte(v) => StoredProperty::Byte(*v),
        PropertyValue::Short(v) => StoredProperty::Short(*v),
        PropertyValue::Int(v) => StoredProperty::Int(*v),
        PropertyValue::Long(v) => StoredProperty::Long(*v),
        PropertyValue::Float(v) => StoredProperty::Float(*v),
        PropertyValue::Double(v) => StoredProperty::Double(*v),
        PropertyValue::NodeId(node_ref) => StoredProperty::NodeId(*node_ref),
        PropertyValue::String(s) => StoredProperty::StringId(strings.intern(s)),
        PropertyValue::Enum(v) => StoredProperty::Enum(*v),
    }
}
