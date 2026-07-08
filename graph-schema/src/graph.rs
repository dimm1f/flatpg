use std::collections::{BTreeMap, HashMap};

use crate::{
    EdgeDirectionKind, ItemAsStr, ItemIndex,
    edge::{Direction, Edge, EdgeHandle, EdgeRef},
    error::Error,
    node::{NewNode, Node, NodeMeta, NodeRef},
    property::PropertyValue,
    schema::{EdgeKind, EdgeStorageSlot, NodeKind, PropKind, Schema},
    storage::{EdgeStorage, NodeMetaStorage, PropertyStorage, StorageArray, StoredProperty},
    strings_pool::{StringRef, StringsPool},
};

pub struct Graph<S> {
    nodes: NodeMetaStorage<S>,
    neighbors: EdgeStorage<S>,
    properties: PropertyStorage<S>,
    strings: StringsPool,
}

impl<S: Schema> Graph<S> {
    pub fn new() -> Self {
        Self {
            nodes: NodeMetaStorage::new(),
            neighbors: EdgeStorage::new(),
            properties: PropertyStorage::new(),
            strings: StringsPool::new(),
        }
    }

    pub fn nodes(&self) -> &NodeMetaStorage<S> {
        &self.nodes
    }

    pub fn neighbors(&self) -> &EdgeStorage<S> {
        &self.neighbors
    }

    pub fn properties(&self) -> &PropertyStorage<S> {
        &self.properties
    }

    pub fn resolve_string(&self, str_ref: StringRef) -> Result<&str, Error> {
        self.strings
            .get(str_ref)
            .ok_or_else(|| Error::unresolved_string_ref(str_ref.to_string()))
    }

    /// Converts a [`StoredProperty`] into a self-contained [`PropertyValue`],
    /// resolving string refs against this graph's strings pool.
    pub fn resolve_property(&self, prop: StoredProperty) -> Result<PropertyValue, Error> {
        Ok(match prop {
            StoredProperty::Bool(v) => PropertyValue::Bool(v),
            StoredProperty::Byte(v) => PropertyValue::Byte(v),
            StoredProperty::Short(v) => PropertyValue::Short(v),
            StoredProperty::Int(v) => PropertyValue::Int(v),
            StoredProperty::Long(v) => PropertyValue::Long(v),
            StoredProperty::Float(v) => PropertyValue::Float(v),
            StoredProperty::Double(v) => PropertyValue::Double(v),
            StoredProperty::NodeRef(v) => PropertyValue::NodeRef(v),
            StoredProperty::StringRef(str_ref) => {
                PropertyValue::String(self.resolve_string(str_ref)?.to_owned())
            }
        })
    }

    pub fn node_count_by_kind(&self, node_kind: NodeKind<S>) -> usize {
        self.nodes[node_kind.index()]
            .iter()
            .filter(|&&node| !node.is_deleted())
            .count()
    }

    pub fn nodes_count(&self) -> usize {
        S::node_kinds()
            .iter()
            .map(|kind| self.node_count_by_kind(*kind))
            .sum()
    }

    pub fn nodes_by_kind(&self, node_kind: NodeKind<S>) -> impl Iterator<Item = Node<S>> {
        self.nodes[node_kind.index()]
            .iter()
            .enumerate()
            .filter(|(_, meta)| !meta.is_deleted())
            .map(move |(seq, _)| Node::<S>::new(node_kind, seq))
    }

    pub fn is_node_deleted(&self, node_ref: Node<S>) -> bool {
        node_is_deleted::<S>(&self.nodes, node_ref)
    }

    pub fn nodes_by_kind_with_deleted(
        &self,
        node_kind: NodeKind<S>,
    ) -> impl Iterator<Item = Node<S>> {
        self.nodes[node_kind.index()]
            .iter()
            .enumerate()
            .map(move |(seq, _)| Node::<S>::new(node_kind, seq))
    }
    // TODO: this and node_count_by_kind need refactoring
    pub fn node_count_by_kind_with_deleted(&self, node_kind: NodeKind<S>) -> usize {
        self.nodes[node_kind.index()].len()
    }

    pub fn get_node_property(
        &self,
        node_ref: NodeRef,
        property_kind: PropKind<S>,
    ) -> Result<impl Iterator<Item = StoredProperty>, Error> {
        let kind = S::resolve_node_kind(node_ref)?;

        let slot = S::property_storage_slot(kind, property_kind);

        let offsets = self
            .properties()
            .get(slot.offset_index())
            .ok_or_else(|| Error::invalid_slot_index(slot.to_string()))
            .and_then(StorageArray::try_as_int)?;

        let indexes = offsets
            .get(node_ref.seq()..=node_ref.seq() + 1)
            .filter(|s| s.len() == 2);

        let Some([start, end]) = indexes else {
            return Err(Error::property_index_not_found());
        };
        let start = *start as usize;
        let end = *end as usize;

        if start > end || end >= self.properties().len() {
            return Err(Error::property_index_out_of_bounds(
                start,
                end,
                self.properties().len(),
            ));
        }

        Ok(self
            .properties()
            .get(slot.values_index())
            .into_iter()
            .flat_map(move |props| (start..end).filter_map(|i| props.get(i))))
    }

    #[inline]
    fn get_edges_offset(&self, node: NodeRef, slot: EdgeStorageSlot) -> Result<(i32, i32), Error> {
        self.neighbors()
            .get(slot.offset_index())
            .ok_or_else(|| Error::slot_offsets_not_found(slot.to_string()))
            .and_then(StorageArray::try_as_int)
            .map(|v| v.get(node.seq()..=(node.seq() + 1)))
            .and_then(|o| {
                if let Some([start, end]) = o {
                    Ok((*start, *end))
                } else {
                    Err(Error::node_offset_not_found(node.seq()))
                }
            })
    }
    pub fn get_edges_count(
        &self,
        node_ref: NodeRef,
        edge_kind: EdgeKind<S>,
        direction: Direction,
    ) -> Result<usize, Error> {
        let kind = S::resolve_node_kind(node_ref)?;
        let slot = S::edge_storage_slot(kind, direction, edge_kind);
        let result = self
            .get_edges_offset(node_ref, slot)
            .map(|(start, end)| end - start)
            .unwrap_or(0);

        if result < 0 {
            Err(Error::invalid_edge_range())
        } else {
            Ok(result as usize)
        }
    }

    pub fn get_edges(
        &self,
        src_node: Node<S>,
        edge_kind: EdgeKind<S>,
        direction: Direction,
    ) -> Result<Vec<Edge<S>>, Error> {
        let slot = S::edge_storage_slot(src_node.kind(), direction, edge_kind);
        let (start, end) = self.get_edges_offset((&src_node).into(), slot)?;

        let start = start as usize;
        let end = end as usize;
        let length = end - start;

        let mut result = Vec::with_capacity(length);

        for i in 0..length {
            let dst_node = self
                .neighbors()
                .get(slot.neighbors_index())
                .and_then(|x| x.get(start + i))
                .and_then(|p| match p {
                    StoredProperty::NodeRef(node_ref) => Some(node_ref),
                    _ => None,
                })
                .ok_or_else(|| Error::neighbor_not_found(start + i))?;

            let edge_handle = EdgeHandle::new(edge_kind.index(), direction.factor(), i);
            let edge =
                S::make_edge((&src_node).into(), dst_node, direction, edge_handle).try_into()?;

            result.push(edge);
        }
        Ok(result)
    }

    /// Returns the property attached to `edge`, or `Ok(None)` when the edge's
    /// kind carries no property value.
    pub fn get_edge_property(&self, edge: Edge<S>) -> Result<Option<PropertyValue>, Error> {
        // `edge.seq()` indexes the adjacency list of the node the edge was queried
        // from; for In-direction edges that node is `dst_node`, not `src_node`.
        let (node_ref, direction, _, _) = edge
            .direction()
            .orient_edge((&edge.src_node()).into(), (&edge.dst_node()).into());

        let node_kind = S::resolve_node_kind(node_ref)?;
        let slot = S::edge_storage_slot(node_kind, direction, edge.kind());
        let (start, _) = self.get_edges_offset(node_ref, slot)?;

        self.neighbors()
            .get(slot.properties_index())
            .and_then(|v| v.get(start as usize + edge.seq()))
            .map(|prop| self.resolve_property(prop))
            .transpose()
    }
}

impl<S: Schema> Default for Graph<S> {
    fn default() -> Self {
        Self::new()
    }
}

type NewEdgeId = usize;

struct HalfEdge<S: Schema> {
    node: Node<S>,
    neighbor: Node<S>,
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

type ChangeId = usize;
enum Change<S: Schema> {
    RemoveNode(NodeRef),
    UpdateNodeProperty(NodeRef, PropKind<S>, QuantifiedProperty),
    RemoveEdge(EdgeRef),
}

// TODO Ambigous with QuantifiedType
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
    Existing(NodeRef),
}

impl From<NewNodeId> for NewOrExistingNode {
    fn from(value: NewNodeId) -> Self {
        Self::New(value)
    }
}

impl From<NodeRef> for NewOrExistingNode {
    fn from(value: NodeRef) -> Self {
        Self::Existing(value)
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

    pub fn remove_node<T: Into<NodeRef>>(&mut self, node_ref: T) -> ChangeId {
        self.changes.push(Change::RemoveNode(node_ref.into()));
        self.changes.len() - 1
    }

    pub fn remove_edge<T: Into<Edge<S>>>(&mut self, edge: T) -> ChangeId {
        let edge: Edge<S> = edge.into();
        self.changes.push(Change::RemoveEdge((&edge).into()));
        self.changes.len() - 1
    }

    pub fn update_node_property<T: Into<NodeRef>, P: Into<QuantifiedProperty>>(
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

    pub fn apply(self, mut graph: Graph<S>) -> Result<Graph<S>, Error> {
        self.apply_changes(&mut graph)?;

        // Note: `node_remapper` must contain nodes with actual NodeSeq.
        // Therefore, the max seq per kind must be obtained from graph before mapping.
        let mut node_remapper: HashMap<NewNodeId, NodeRef> = HashMap::new();
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

        let mut new_nodes = NodeMetaStorage::new();
        let mut new_properties = PropertyStorage::new();
        let mut new_edges = EdgeStorage::new();

        for (node_kind, property_kind) in S::property_storage_slots_iter() {
            let slot = S::property_storage_slot(node_kind, property_kind);

            // Safety: new_properties has property_storage_size() slots; slot.offset_index() is always in-bounds.
            let offsets = unsafe { new_properties.get_unchecked_mut(slot.offset_index()) }
                .try_as_int_mut()?;
            *offsets = vec![0; new_nodes_count[node_kind.index()] + 1];
        }

        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

            // Safety: new_edges has edge_storage_size() slots; slot.offset_index() is always in-bounds.
            let offsets =
                unsafe { new_edges.get_unchecked_mut(slot.offset_index()) }.try_as_int_mut()?;
            *offsets = vec![0; new_nodes_count[node_kind.index()] + 1];
        }

        let mut seq_counters = vec![0usize; S::number_of_node_kinds()];

        let mut slot_property = HashMap::new();

        for (i, node) in self.new_nodes.iter().enumerate() {
            // Safety: seq_counters has number_of_node_kinds() elements; node.kind().index() is always in-bounds.
            let current_seq = unsafe { seq_counters.get_unchecked_mut(node.kind().index()) };

            let local_index = *current_seq;
            let seq = local_index + graph_nodes_max_seq[node.kind().index()];
            *current_seq += 1;

            // Safety: new_nodes has number_of_node_kinds() slots; node.kind().index() is always in-bounds.
            let nodes_storage = unsafe { new_nodes.get_unchecked_mut(node.kind().index()) };
            nodes_storage.push(NodeMeta::default());

            node_remapper.insert(i, NodeRef::new(node.kind().index(), seq));

            for (prop_kind, new_values) in node.properties() {
                slot_property
                    .entry((node.kind(), *prop_kind))
                    .or_insert_with(BTreeMap::new)
                    .insert(local_index, new_values);
            }
        }

        for ((node_kind, property_kind), seq_property) in slot_property {
            let slot = S::property_storage_slot(node_kind, property_kind);

            // Safety: new_properties has property_storage_size() slots; slot guarantees both indices are in-bounds and distinct.
            let [offsets, storage] = unsafe {
                new_properties
                    .get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
            };

            let offsets = offsets.try_as_int_mut()?;

            let mut delta = 0;

            #[allow(clippy::needless_range_loop)]
            for end in 1..offsets.len() {
                let start = end - 1;

                if let Some(props) = seq_property.get(&start) {
                    let place = offsets[end] as usize + delta;
                    for (i, prop) in props.iter().enumerate() {
                        let prop = &to_stored_property(prop, &mut graph.strings);
                        storage.try_insert(place + i, prop)?;
                    }
                    delta += props.len();
                }

                offsets[end] += delta as i32
            }
        }

        // Append new items into the graph
        graph.nodes.append(new_nodes);
        graph.properties.append(new_properties);
        // Initialize the offsers array with new nodes offsets
        graph.neighbors.append(new_edges);

        let resolve_node_ref = |node: &NewOrExistingNode| -> Option<NodeRef> {
            match node {
                NewOrExistingNode::New(id) => node_remapper.get(id).copied(),
                NewOrExistingNode::Existing(node_ref) => Some(*node_ref),
            }
        };

        // WARN: Edges are inserted directly into the graph so any issues at this stage can corrupt the graph

        let slot_edge_halves = self
            .new_edges
            .iter()
            .filter_map(|new_edge| {
                let property = new_edge
                    .property
                    .as_ref()
                    .map(|prop| to_stored_property(prop, &mut graph.strings));
                edge_to_halves(new_edge, resolve_node_ref, property)
            })
            // Access `graph.nodes` directly (rather than through the `graph.is_node_deleted`
            // method, which would borrow all of `graph`) so this closure's borrow stays
            // disjoint from the `&mut graph.strings` borrow captured by the closure above.
            .filter(|halves| {
                halves
                    .iter()
                    .all(|h| !node_is_deleted::<S>(&graph.nodes, h.node))
            })
            .flatten()
            .fold(HashMap::new(), |mut acc, half| {
                acc.entry((half.node.kind(), half.direction, half.edge_kind))
                    .or_insert_with(BTreeMap::new)
                    .entry(half.node.seq())
                    .or_insert_with(Vec::new)
                    .push(half);
                acc
            });

        for ((node_kind, direction, edge_kind), seq_halves) in slot_edge_halves {
            let slot = S::edge_storage_slot(node_kind, direction, edge_kind);
            // Safety: graph.neighbors_mut() covers all schema edge slots (including new nodes appended above); slot guarantees all
            // three indices are in-bounds and pairwise distinct.
            let [offsets, neigbors, properties] = unsafe {
                graph.neighbors.get_disjoint_unchecked_mut([
                    slot.offset_index(),
                    slot.neighbors_index(),
                    slot.properties_index(),
                ])
            };

            let offsets = offsets.try_as_int_mut()?;
            let neigbors = neigbors.try_as_ref_mut()?;

            let mut delta = 0;

            #[allow(clippy::needless_range_loop)]
            for end in 1..offsets.len() {
                let start = end - 1;

                if let Some(halves) = seq_halves.get(&start) {
                    let new_neighbors = halves.iter().map(|h| NodeRef::from(&h.neighbor));

                    let place = offsets[end] as usize + delta;
                    neigbors.splice(place..place, new_neighbors);
                    delta += halves.len();

                    for (i, half) in halves.iter().enumerate() {
                        if let Some(prop) = &half.property {
                            properties.try_insert(place + i, prop)?;
                        }
                    }
                }

                offsets[end] += delta as i32
            }
        }

        Ok(graph)
    }

    // TODO: Needs refactoring
    fn apply_changes(&self, graph: &mut Graph<S>) -> Result<(), Error> {
        for change in &self.changes {
            match change {
                Change::RemoveNode(node_ref) => {
                    let node: Node<S> = (*node_ref).try_into()?;
                    if let Some(seq) = graph.nodes[node.kind().index()].get_mut(node_ref.seq()) {
                        seq.set_is_deleted(true);
                    }
                }
                Change::UpdateNodeProperty(node_ref, property_kind, quantified_property) => {
                    let node: Node<S> = (*node_ref).try_into()?;
                    let slot = S::property_storage_slot(node.kind(), *property_kind);

                    let new_values: &[PropertyValue] = match quantified_property {
                        QuantifiedProperty::One(p) => std::slice::from_ref(p),
                        QuantifiedProperty::Multi(ps) => ps.as_slice(),
                    };

                    // Intern any strings before taking the disjoint borrow of graph.properties
                    // below, so the two mutable borrows never need to overlap.
                    let stored_values: Vec<StoredProperty> = new_values
                        .iter()
                        .map(|prop| to_stored_property(prop, &mut graph.strings))
                        .collect();

                    // Safety: graph.properties_mut() covers all schema property slots; slot guarantees both indices are in-bounds and distinct.
                    let [offsets_arr, values_arr] = unsafe {
                        graph
                            .properties
                            .get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
                    };

                    let offsets = offsets_arr.try_as_int_mut()?;
                    let start = offsets[node_ref.seq()] as usize;
                    let end = offsets[node_ref.seq() + 1] as usize;
                    let old_count = (end - start) as i32;
                    let delta = stored_values.len() as i32 - old_count;

                    values_arr.try_drain(start..end)?;
                    for (i, prop) in stored_values.iter().enumerate() {
                        values_arr.try_insert(start + i, prop)?;
                    }

                    #[allow(clippy::needless_range_loop)]
                    for i in (node_ref.seq() + 1)..offsets.len() {
                        offsets[i] += delta;
                    }
                }
                Change::RemoveEdge(edge) => {
                    let src = edge.src_node_ref();
                    let dst = edge.dst();
                    let edge_kind = S::resolve_edge_kind(edge.handle())?;
                    let seq = edge.handle().seq();

                    let (primary, primary_dir, secondary, secondary_dir) =
                        S::resolve_edge_direction(edge.handle())?.orient_edge(src, dst);

                    // Find the secondary position before modifying the graph
                    let secondary_seq =
                        find_reverse_edge_seq(graph, secondary, secondary_dir, edge_kind, primary)?;

                    remove_half_edge(graph, primary, primary_dir, edge_kind, seq)?;
                    remove_half_edge(graph, secondary, secondary_dir, edge_kind, secondary_seq)?;
                }
            }
        }
        Ok(())
    }
}

fn node_is_deleted<S: Schema>(nodes: &NodeMetaStorage<S>, node_ref: Node<S>) -> bool {
    S::node_kind_by_index(node_ref.kind().index())
        .and_then(|kind| {
            nodes[kind.index()]
                .get(node_ref.seq())
                .map(NodeMeta::is_deleted)
        })
        .unwrap_or(true)
}

fn remove_half_edge<S>(
    graph: &mut Graph<S>,
    node_ref: NodeRef,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    local_seq: usize,
) -> Result<(), Error>
where
    S: Schema,
{
    let node_kind = S::resolve_node_kind(node_ref)?;
    let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

    // Safety: graph.neighbors_mut() covers all schema edge slots; slot guarantees all three indices are in-bounds
    // and pairwise distinct. local_seq is within the node's adjacency range as validated by the caller.
    let [offsets_arr, neighbors_arr, properties_arr] = unsafe {
        graph.neighbors.get_disjoint_unchecked_mut([
            slot.offset_index(),
            slot.neighbors_index(),
            slot.properties_index(),
        ])
    };

    let offsets = offsets_arr.try_as_int_mut()?;
    let start = offsets[node_ref.seq()] as usize;
    let idx = start + local_seq;

    neighbors_arr.try_drain(idx..idx + 1)?;
    properties_arr.try_drain(idx..idx + 1)?;

    #[allow(clippy::needless_range_loop)]
    for i in (node_ref.seq() + 1)..offsets.len() {
        offsets[i] -= 1;
    }

    Ok(())
}

fn find_reverse_edge_seq<S>(
    graph: &Graph<S>,
    node: NodeRef,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    target: NodeRef,
) -> Result<usize, Error>
where
    S: Schema,
{
    let node: Node<S> = node.try_into()?;
    let slot = S::edge_storage_slot(node.kind(), direction, edge_kind);

    let offsets = graph
        .neighbors()
        .get(slot.offset_index())
        .ok_or_else(|| Error::invalid_slot_index(slot.to_string()))?
        .try_as_int()?;

    let start = offsets[node.seq()] as usize;
    let end = offsets[node.seq() + 1] as usize;

    let neighbors = graph
        .neighbors()
        .get(slot.neighbors_index())
        .ok_or_else(|| Error::neighbor_not_found(slot.neighbors_index()))?
        .try_as_ref()?;

    neighbors[start..end]
        .iter()
        .position(|&n| n == target)
        .ok_or_else(|| match target.try_into() {
            Ok::<Node<S>, _>(target) => Error::reverse_edge_not_found(
                target.to_string(),
                node.to_string(),
                direction.as_str().to_owned(),
                edge_kind.as_str().to_owned(),
            ),
            Err(e) => e,
        })
}

fn edge_to_halves<F, S>(
    new_edge: &NewEdge<S>,
    node_resolver: F,
    property: Option<StoredProperty>,
) -> Option<[HalfEdge<S>; 2]>
where
    F: Fn(&NewOrExistingNode) -> Option<NodeRef>,
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

fn to_stored_property(prop: &PropertyValue, strings: &mut StringsPool) -> StoredProperty {
    match prop {
        PropertyValue::Bool(v) => StoredProperty::Bool(*v),
        PropertyValue::Byte(v) => StoredProperty::Byte(*v),
        PropertyValue::Short(v) => StoredProperty::Short(*v),
        PropertyValue::Int(v) => StoredProperty::Int(*v),
        PropertyValue::Long(v) => StoredProperty::Long(*v),
        PropertyValue::Float(v) => StoredProperty::Float(*v),
        PropertyValue::Double(v) => StoredProperty::Double(*v),
        PropertyValue::NodeRef(node_ref) => StoredProperty::NodeRef(*node_ref),
        PropertyValue::String(s) => StoredProperty::StringRef(strings.intern(s)),
    }
}
