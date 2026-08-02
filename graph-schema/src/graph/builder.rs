use std::collections::{BTreeMap, HashMap};

use crate::{
    EdgeDirectionKind, ItemAsStr, ItemIndex,
    edge::{Direction, EdgeId, RawEdgeId},
    error::Error,
    graph::{Graph, GraphView, node_is_deleted},
    node::{NewNode, NodeId, NodeMeta, RawNodeId},
    property::{PropertyType, PropertyValue},
    schema::{EdgeKind, PropKind, Schema},
    storage::{
        EdgeStorage, NodeMetaStorage, Offset, PropertyStorage, StorageArray, StoredProperty,
    },
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

    pub fn apply(self, graph: impl GraphView<S>) -> Result<Graph<S>, Error> {
        let mut graph = graph.into_graph();
        self.apply_changes(&mut graph)?;

        // Note: `node_remapper` must contain nodes with actual NodeSeq.
        // Therefore, the max seq per kind must be obtained from graph before mapping.
        let mut node_remapper: HashMap<NewNodeId, RawNodeId> = HashMap::new();
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
                .try_as_offset_mut()?;
            *offsets = vec![Offset::zero(); new_nodes_count[node_kind.index()] + 1];
        }

        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

            // Safety: new_edges has edge_storage_size() slots; slot.offset_index() is always in-bounds.
            let offsets =
                unsafe { new_edges.get_unchecked_mut(slot.offset_index()) }.try_as_offset_mut()?;
            *offsets = vec![Offset::zero(); new_nodes_count[node_kind.index()] + 1];
        }

        let mut seq_counters = vec![0usize; S::number_of_node_kinds()];

        let mut slot_property = BTreeMap::new();

        for (i, node) in self.new_nodes.iter().enumerate() {
            // Safety: seq_counters has number_of_node_kinds() elements; node.kind().index() is always in-bounds.
            let current_seq = unsafe { seq_counters.get_unchecked_mut(node.kind().index()) };

            let local_index = *current_seq;
            let seq = local_index + graph_nodes_max_seq[node.kind().index()];
            *current_seq += 1;

            // Safety: new_nodes has number_of_node_kinds() slots; node.kind().index() is always in-bounds.
            let nodes_storage = unsafe { new_nodes.get_unchecked_mut(node.kind().index()) };
            nodes_storage.push(NodeMeta::default());

            node_remapper.insert(i, RawNodeId::new(node.kind().index(), seq));

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

            let offsets = offsets.try_as_offset_mut()?;

            let mut delta = 0;

            #[allow(clippy::needless_range_loop)]
            for end in 1..offsets.len() {
                let start = end - 1;

                if let Some(props) = seq_property.get(&start) {
                    let place = offsets[end].value() + delta;

                    let mut batch = StorageArray::with_capacity(storage.typ(), props.len());
                    for prop in props.iter() {
                        let prop = to_stored_property(prop, &mut graph.strings);
                        batch.try_push(&prop)?;
                    }
                    delta += props.len();
                    storage.try_splice(place, batch)?;
                }

                offsets[end] = offsets[end].checked_add_delta(delta)?;
            }
        }

        // Append new items into the graph
        graph.node_meta_storage.append(new_nodes);
        graph.property_storage.append(new_properties)?;
        // Initialize the offsers array with new nodes offsets
        graph.edge_storage.append(new_edges)?;

        let resolve_node_ref = |node: &NewOrExistingNode| -> Option<RawNodeId> {
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
            // Access `graph.node_meta_storage` directly (rather than through the
            // `graph.is_node_deleted` method, which would borrow all of `graph`) so this
            // closure's borrow stays disjoint from the `&mut graph.strings` borrow
            // captured by the closure above.
            .filter(|halves| {
                halves
                    .iter()
                    .all(|h| !node_is_deleted::<S>(&graph.node_meta_storage, h.node))
            })
            .flatten()
            .fold(BTreeMap::new(), |mut acc, half| {
                acc.entry((half.node.kind(), half.direction, half.edge_kind))
                    .or_insert_with(BTreeMap::new)
                    .entry(half.node.seq())
                    .or_insert_with(Vec::new)
                    .push(half);
                acc
            });

        for ((node_kind, direction, edge_kind), seq_halves) in slot_edge_halves {
            let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

            // Safety: graph.edge_storage covers all schema edge slots (including new nodes appended above); slot guarantees all
            // three indices are in-bounds and pairwise distinct.
            let [offsets, neigbors, properties] = unsafe {
                graph.edge_storage.get_disjoint_unchecked_mut([
                    slot.offset_index(),
                    slot.neighbors_index(),
                    slot.properties_index(),
                ])
            };

            let offsets = offsets.try_as_offset_mut()?;
            let neigbors = neigbors.try_as_node_id_mut()?;

            let mut delta = 0;

            #[allow(clippy::needless_range_loop)]
            for end in 1..offsets.len() {
                let start = end - 1;

                if let Some(halves) = seq_halves.get(&start) {
                    let new_neighbors = halves.iter().map(|h| RawNodeId::from(&h.neighbor));

                    let place = offsets[end].value() + delta;
                    neigbors.splice(place..place, new_neighbors);
                    delta += halves.len();

                    if properties.typ() != PropertyType::None {
                        let mut batch = StorageArray::with_capacity(properties.typ(), halves.len());
                        for half in halves.iter() {
                            if let Some(prop) = &half.property {
                                batch.try_push(prop)?;
                            }
                        }
                        properties.try_splice(place, batch)?;
                    }
                }

                offsets[end] = offsets[end].checked_add_delta(delta)?;
            }
        }

        Ok(graph)
    }

    fn apply_changes(&self, graph: &mut Graph<S>) -> Result<(), Error> {
        for change in &self.changes {
            match change {
                Change::RemoveNode(node_ref) => {
                    let node: NodeId<S> = (*node_ref).try_into()?;
                    if let Some(seq) =
                        graph.node_meta_storage[node.kind().index()].get_mut(node_ref.seq())
                    {
                        seq.set_is_deleted(true);
                    }
                }
                Change::UpdateNodeProperty(node_ref, property_kind, quantified_property) => {
                    let node: NodeId<S> = (*node_ref).try_into()?;
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

                    // Safety: graph.property_storage covers all schema property slots; slot guarantees both indices are in-bounds and distinct.
                    let [offsets_arr, values_arr] = unsafe {
                        graph
                            .property_storage
                            .get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
                    };

                    let offsets = offsets_arr.try_as_offset_mut()?;
                    let start = offsets[node_ref.seq()];
                    let end = offsets[node_ref.seq() + 1];
                    let old_count = end.checked_sub(start)?;
                    let new_count = stored_values.len();

                    values_arr.try_drain(start.value()..end.value())?;
                    for (i, prop) in stored_values.iter().enumerate() {
                        values_arr.try_insert(start.value() + i, prop)?;
                    }

                    #[allow(clippy::needless_range_loop)]
                    for i in (node_ref.seq() + 1)..offsets.len() {
                        offsets[i] = if new_count >= old_count {
                            offsets[i].checked_add_delta(new_count - old_count)?
                        } else {
                            offsets[i].checked_sub_delta(old_count - new_count)?
                        };
                    }
                }
                Change::RemoveEdge(edge) => {
                    let src = edge.src_node_id();
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

fn remove_half_edge<S>(
    graph: &mut Graph<S>,
    node_ref: RawNodeId,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    local_seq: usize,
) -> Result<(), Error>
where
    S: Schema,
{
    let node_kind = S::resolve_node_kind(node_ref)?;
    let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

    // Safety: graph.edge_storage covers all schema edge slots; slot guarantees all three indices are in-bounds
    // and pairwise distinct. local_seq is within the node's adjacency range as validated by the caller.
    let [offsets_arr, neighbors_arr, properties_arr] = unsafe {
        graph.edge_storage.get_disjoint_unchecked_mut([
            slot.offset_index(),
            slot.neighbors_index(),
            slot.properties_index(),
        ])
    };

    let offsets = offsets_arr.try_as_offset_mut()?;
    let start = offsets[node_ref.seq()].value();
    let idx = start + local_seq;

    neighbors_arr.try_drain(idx..idx + 1)?;
    properties_arr.try_drain(idx..idx + 1)?;

    #[allow(clippy::needless_range_loop)]
    for i in (node_ref.seq() + 1)..offsets.len() {
        offsets[i] = offsets[i].checked_sub_delta(1)?;
    }

    Ok(())
}

fn find_reverse_edge_seq<S>(
    graph: &Graph<S>,
    node: RawNodeId,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    target: RawNodeId,
) -> Result<usize, Error>
where
    S: Schema,
{
    let node: NodeId<S> = node.try_into()?;
    let slot = S::edge_storage_slot(node.kind(), direction, edge_kind);

    let offsets = graph
        .edge_storage
        .get(slot.offset_index())
        .ok_or_else(|| Error::invalid_slot_index(slot.to_string()))?
        .try_as_offset()?;

    let start = offsets[node.seq()].value();
    let end = offsets[node.seq() + 1].value();

    let neighbors = graph
        .edge_storage
        .get(slot.neighbors_index())
        .ok_or_else(|| Error::neighbor_not_found(slot.neighbors_index()))?
        .try_as_node_id()?;

    neighbors[start..end]
        .iter()
        .position(|&n| n == target)
        .ok_or_else(|| match target.try_into() {
            Ok::<NodeId<S>, _>(target) => Error::reverse_edge_not_found(
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
