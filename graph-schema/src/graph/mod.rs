use crate::{
    EdgeDirectionKind, ItemIndex,
    edge::{Direction, EdgeId},
    error::Error,
    graph::integrity::{CheckIntegrity, check_integrity},
    node::{NodeId, NodeMeta, RawNodeId},
    property::PropertyValue,
    schema::{EdgeKind, NodeKind, PropKind, Schema},
    storage::{
        EdgePropertyStorage, EdgeSeq, EdgeStorage, EdgeStorageSlot, NodeMetaStorage, Offset,
        OffsetStorage, PropertyStorage, StorageArrayIter, StoredProperty,
    },
    strings_pool::{RawStringId, StringsPool},
};

pub mod builder;
mod compact;
pub mod integrity;
pub mod raw;

pub struct Graph<S> {
    node_meta_storage: NodeMetaStorage<S>,
    edge_storage: EdgeStorage<S>,
    property_storage: PropertyStorage<S>,
    edge_property_storage: EdgePropertyStorage<S>,
    strings: StringsPool,
}

impl<S: Schema> Graph<S> {
    pub fn new() -> Self {
        Self {
            node_meta_storage: NodeMetaStorage::new(),
            edge_storage: EdgeStorage::new(),
            property_storage: PropertyStorage::new(),
            edge_property_storage: EdgePropertyStorage::new(),
            strings: StringsPool::new(),
        }
    }

    pub fn resolve_string(&self, string_id: RawStringId) -> Result<&str, Error> {
        self.strings
            .get(string_id)
            .ok_or_else(|| Error::unresolved_string_id(string_id.to_string()))
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
            StoredProperty::NodeId(v) => PropertyValue::NodeId(v),
            StoredProperty::StringId(string_id) => {
                PropertyValue::String(self.resolve_string(string_id)?.to_owned())
            }
            StoredProperty::Enum(v) => PropertyValue::Enum(v),
        })
    }

    // Perf: this is an O(n) scan over the kind's node metadata, recomputed on every call.
    pub fn node_count_by_kind(&self, node_kind: NodeKind<S>) -> usize {
        self.node_meta_storage[node_kind.index()]
            .iter()
            .filter(|&&node| !node.is_deleted())
            .count()
    }

    // Perf: sums `node_count_by_kind` across all kinds, so this is O(total nodes), recomputed
    // on every call
    pub fn node_count(&self) -> usize {
        S::node_kinds()
            .iter()
            .map(|kind| self.node_count_by_kind(*kind))
            .sum()
    }

    pub fn nodes_by_kind(&self, node_kind: NodeKind<S>) -> impl Iterator<Item = NodeId<S>> {
        self.node_meta_storage[node_kind.index()]
            .iter()
            .enumerate()
            .filter(|(_, meta)| !meta.is_deleted())
            .map(move |(seq, _)| NodeId::<S>::new(node_kind, seq))
    }

    pub fn is_node_deleted(&self, node_ref: NodeId<S>) -> bool {
        node_is_deleted::<S>(&self.node_meta_storage, node_ref)
    }

    pub fn nodes_by_kind_with_deleted(
        &self,
        node_kind: NodeKind<S>,
    ) -> impl Iterator<Item = NodeId<S>> {
        self.node_meta_storage[node_kind.index()]
            .iter()
            .enumerate()
            .map(move |(seq, _)| NodeId::<S>::new(node_kind, seq))
    }

    pub fn node_count_by_kind_with_deleted(&self, node_kind: NodeKind<S>) -> usize {
        self.node_meta_storage[node_kind.index()].len()
    }

    pub fn get_node_property(
        &self,
        node_ref: RawNodeId,
        property_kind: PropKind<S>,
    ) -> Result<impl Iterator<Item = StoredProperty>, Error> {
        let kind = S::resolve_node_kind(node_ref)?;

        let slot_index = S::property_storage_slot(kind, property_kind);

        let slot = &self.property_storage[slot_index.index()];

        let indexes = slot.get_offset(node_ref.seq());

        let Some((start, end)) = indexes else {
            return Err(Error::property_index_not_found());
        };

        if start > end || end.value() > slot.values().len() {
            return Err(Error::property_index_out_of_bounds(
                start.value(),
                end.value(),
                slot.values().len(),
            ));
        }

        Ok(slot.get_values(start, end))
    }

    #[inline]
    fn get_edges_offset(
        &self,
        node: RawNodeId,
        slot: &EdgeStorageSlot,
    ) -> Result<(Offset, Offset), Error> {
        if slot.offsets().is_empty() {
            return Ok((Offset::zero(), Offset::zero()));
        }

        match slot.get_offset(node.seq()) {
            Some((start, end)) => Ok((start, end)),
            _ => Err(Error::node_offset_not_found(node.seq())),
        }
    }
    pub fn get_edges_count(
        &self,
        node_ref: RawNodeId,
        edge_kind: EdgeKind<S>,
        direction: Direction,
    ) -> Result<usize, Error> {
        let kind = S::resolve_node_kind(node_ref)?;
        let slot = &self.edge_storage[S::edge_storage_slot(kind, direction, edge_kind).index()];

        match self.get_edges_offset(node_ref, slot) {
            Ok((start, end)) => end.checked_sub(start),
            Err(_) => Ok(0),
        }
    }

    /// Returns `src_node`'s `edge_kind` half-edges for `direction`.
    ///
    /// The iterator borrows the adjacency list in place and allocates nothing.
    ///
    /// # Panics
    ///
    /// Panics if a neighbor's stored node kind is not part of the schema. Every way of
    /// building a [`Graph`] rejects such a neighbor first, so a graph obtained through this
    /// crate's API cannot hold one.
    pub fn get_edges(
        &self,
        src_node: NodeId<S>,
        edge_kind: EdgeKind<S>,
        direction: Direction,
    ) -> Result<impl ExactSizeIterator<Item = EdgeId<S>>, Error> {
        let slot_index = S::edge_storage_slot(src_node.kind(), direction, edge_kind);
        let slot = &self.edge_storage[slot_index.index()];
        let (start, end) = self.get_edges_offset((&src_node).into(), slot)?;

        // `get_neighbors` clamps a reversed range to an empty slice, so without this check a
        // broken offsets array would read as "this node has no edges" rather than as an error.
        end.checked_sub(start)?;

        Ok(slot
            .get_neighbors(start, end)
            .enumerate()
            .map(move |(seq, neighbor)| {
                // Panic: every path into a `Graph<S>` validates neighbor kinds — `GraphDiff::apply`
                // drops an edge whose endpoint does not resolve, and `TryFrom<RawGraph<S>>` runs
                // `check_integrity`, which resolves the kind of every neighbor via `check_node_id`.
                let neighbor = neighbor
                    .try_into()
                    .expect("neighbor kind checked on graph construction");

                // Absent exactly for kinds carrying no property; the region is already in cache
                // from reading the neighbor beside it.
                let edge_seq = slot.edges().get(start.value() + seq).copied();

                EdgeId::from_half(src_node, neighbor, edge_kind, direction, seq, edge_seq)
            }))
    }

    /// Locates one half-edge's slot and its absolute position within that slot's arrays.
    ///
    /// `seq` indexes the adjacency list of the node the edge was read from; for In-direction
    /// edges that node is `dst`, not `src`, which is what `orient_edge` sorts out here.
    fn locate_half_edge(
        &self,
        src: RawNodeId,
        dst: RawNodeId,
        edge_kind: EdgeKind<S>,
        direction: Direction,
        seq: usize,
    ) -> Result<(&EdgeStorageSlot, Offset), Error> {
        let (node_ref, direction, _, _) = direction.orient_edge(src, dst);

        let node_kind = S::resolve_node_kind(node_ref)?;
        let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind);
        let slot = &self.edge_storage[slot_index.index()];
        let (start, end) = self.get_edges_offset(node_ref, slot)?;

        let position = Offset::new(start.value() + seq)?;
        if position >= end {
            return Err(Error::property_index_out_of_bounds(
                position.value(),
                end.value(),
                slot.neighbors().len(),
            ));
        }
        Ok((slot, position))
    }

    /// Returns the identity of one half-edge, or `None` when its kind carries no property and
    /// so allocates none, or when the position no longer resolves.
    pub fn half_edge_seq(
        &self,
        src: RawNodeId,
        dst: RawNodeId,
        edge_kind: EdgeKind<S>,
        direction: Direction,
        seq: usize,
    ) -> Option<EdgeSeq> {
        let (slot, position) = self
            .locate_half_edge(src, dst, edge_kind, direction, seq)
            .ok()?;
        slot.get_edge_seq(position)
    }

    /// Returns the single raw property value attached to `edge`, for a kind declared
    /// `quantity = One`.
    ///
    /// `Ok(None)` when the kind carries no property. This is the read the typed accessors of
    /// `One` kinds use: it costs one indexed load where [`Graph::get_edge_property`] has to
    /// build an iterator, which is most of the cost of fetching a single value. A `Multi` kind
    /// yields `Ok(None)` here and must use that method instead.
    pub fn get_edge_property_one(&self, edge: EdgeId<S>) -> Result<Option<StoredProperty>, Error> {
        let (slot, position) = self.locate_half_edge(
            (&edge.src_node()).into(),
            (&edge.dst_node()).into(),
            edge.kind(),
            edge.direction(),
            edge.seq(),
        )?;

        let Some(edge_seq) = slot.get_edge_seq(position) else {
            return Ok(None);
        };

        Ok(self.edge_property_storage[edge.kind().index()].get_one(edge_seq))
    }

    /// Returns the raw property values attached to `edge`.
    ///
    /// The iterator is empty when the edge's kind carries no property; a `One`-quantity kind
    /// yields exactly one value and a `Multi` kind as many as the edge carries.
    pub fn get_edge_property(
        &self,
        edge: EdgeId<S>,
    ) -> Result<impl Iterator<Item = StoredProperty>, Error> {
        let (slot, position) = self.locate_half_edge(
            (&edge.src_node()).into(),
            (&edge.dst_node()).into(),
            edge.kind(),
            edge.direction(),
            edge.seq(),
        )?;

        // An edge kind carrying no property allocates no `EdgeSeq`, so there is nothing to
        // resolve: the empty `edges` array is the schema's `PropertyType::None` made concrete.
        let Some(edge_seq) = slot.get_edge_seq(position) else {
            return Ok(StorageArrayIter::Empty);
        };

        self.edge_property_storage[edge.kind().index()].get(edge_seq)
    }
}

impl<S: Schema> Default for Graph<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Schema> CheckIntegrity<S> for Graph<S> {
    fn check_integrity(&self) -> Result<(), Error> {
        check_integrity(
            &self.node_meta_storage,
            &self.edge_storage,
            &self.property_storage,
            &self.edge_property_storage,
            &self.strings,
        )
    }
}

pub trait GraphView<S: Schema> {
    fn graph(&self) -> &Graph<S>;

    /// Consumes this view, returning the [`Graph<S>`] it owns or wraps.
    fn into_graph(self) -> Graph<S>
    where
        Self: Sized;
}

impl<S: Schema> GraphView<S> for Graph<S> {
    fn graph(&self) -> &Graph<S> {
        self
    }

    fn into_graph(self) -> Graph<S> {
        self
    }
}

pub trait GraphViewMut<S: Schema>: GraphView<S> {
    fn graph_mut(&mut self) -> &mut Graph<S>;
}

impl<S: Schema> GraphViewMut<S> for Graph<S> {
    fn graph_mut(&mut self) -> &mut Graph<S> {
        self
    }
}

fn node_is_deleted<S: Schema>(nodes: &NodeMetaStorage<S>, node_ref: NodeId<S>) -> bool {
    nodes[node_ref.kind().index()]
        .get(node_ref.seq())
        .map(NodeMeta::is_deleted)
        .unwrap_or(true)
}
