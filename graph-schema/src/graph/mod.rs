use crate::{
    EdgeDirectionKind, ItemIndex,
    edge::{Direction, EdgeHandle, EdgeId},
    error::Error,
    graph::integrity::{CheckIntegrity, check_integrity},
    node::{NodeId, NodeMeta, RawNodeId},
    property::PropertyValue,
    schema::{EdgeKind, EdgeStorageSlot, NodeKind, PropKind, Schema},
    storage::{
        EdgeStorage, NodeMetaStorage, Offset, PropertyStorage, StorageArray, StoredProperty,
    },
    strings_pool::{RawStringId, StringsPool},
};

pub mod builder;
pub mod integrity;
pub mod raw;

pub struct Graph<S> {
    node_meta_storage: NodeMetaStorage<S>,
    edge_storage: EdgeStorage<S>,
    property_storage: PropertyStorage<S>,
    strings: StringsPool,
}

impl<S: Schema> Graph<S> {
    pub fn new() -> Self {
        Self {
            node_meta_storage: NodeMetaStorage::new(),
            edge_storage: EdgeStorage::new(),
            property_storage: PropertyStorage::new(),
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

        let slot = S::property_storage_slot(kind, property_kind);

        let offsets = self
            .property_storage
            .get(slot.offset_index())
            .ok_or_else(|| Error::invalid_slot_index(slot.to_string()))
            .and_then(StorageArray::try_as_offset)?;

        let indexes = offsets
            .get(node_ref.seq()..=node_ref.seq() + 1)
            .filter(|s| s.len() == 2);

        let Some([start, end]) = indexes else {
            return Err(Error::property_index_not_found());
        };
        let start = start.value();
        let end = end.value();

        if start > end || end >= self.property_storage.len() {
            return Err(Error::property_index_out_of_bounds(
                start,
                end,
                self.property_storage.len(),
            ));
        }

        let values = self
            .property_storage
            .get(slot.values_index())
            .ok_or_else(|| Error::invalid_slot_index(slot.to_string()))?;

        Ok(values.iter_range(start..end))
    }

    #[inline]
    fn get_edges_offset(
        &self,
        node: RawNodeId,
        slot: EdgeStorageSlot,
    ) -> Result<(Offset, Offset), Error> {
        self.edge_storage
            .get(slot.offset_index())
            .ok_or_else(|| Error::slot_offsets_not_found(slot.to_string()))
            .and_then(StorageArray::try_as_offset)
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
        node_ref: RawNodeId,
        edge_kind: EdgeKind<S>,
        direction: Direction,
    ) -> Result<usize, Error> {
        let kind = S::resolve_node_kind(node_ref)?;
        let slot = S::edge_storage_slot(kind, direction, edge_kind);

        match self.get_edges_offset(node_ref, slot) {
            Ok((start, end)) => end.checked_sub(start),
            Err(_) => Ok(0),
        }
    }

    pub fn get_edges(
        &self,
        src_node: NodeId<S>,
        edge_kind: EdgeKind<S>,
        direction: Direction,
    ) -> Result<Vec<EdgeId<S>>, Error> {
        let slot = S::edge_storage_slot(src_node.kind(), direction, edge_kind);
        let (start, end) = self.get_edges_offset((&src_node).into(), slot)?;

        let length = end.checked_sub(start)?;
        let start = start.value();

        let neighbors = self
            .edge_storage
            .get(slot.neighbors_index())
            .ok_or_else(|| Error::neighbor_not_found(start))?
            .try_as_node_id()?;
        let neighbors = neighbors
            .get(start..start + length)
            .ok_or_else(|| Error::neighbor_not_found(start + length))?;

        let mut result = Vec::with_capacity(length);

        for (i, &dst_node) in neighbors.iter().enumerate() {
            let edge_handle = EdgeHandle::new(edge_kind.index(), direction.factor(), i);
            let edge =
                S::make_edge((&src_node).into(), dst_node, direction, edge_handle).try_into()?;

            result.push(edge);
        }
        Ok(result)
    }

    /// Returns the raw property attached to `edge`, or `Ok(None)` when the edge's
    /// kind carries no property value.
    pub fn get_edge_property(&self, edge: EdgeId<S>) -> Result<Option<StoredProperty>, Error> {
        // `edge.seq()` indexes the adjacency list of the node the edge was queried
        // from; for In-direction edges that node is `dst_node`, not `src_node`.
        let (node_ref, direction, _, _) = edge
            .direction()
            .orient_edge((&edge.src_node()).into(), (&edge.dst_node()).into());

        let node_kind = S::resolve_node_kind(node_ref)?;
        let slot = S::edge_storage_slot(node_kind, direction, edge.kind());
        let (start, _) = self.get_edges_offset(node_ref, slot)?;

        Ok(self
            .edge_storage
            .get(slot.properties_index())
            .and_then(|v| v.get(start.value() + edge.seq())))
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

fn node_is_deleted<S: Schema>(nodes: &NodeMetaStorage<S>, node_ref: NodeId<S>) -> bool {
    S::node_kind_by_index(node_ref.kind().index())
        .and_then(|kind| {
            nodes[kind.index()]
                .get(node_ref.seq())
                .map(NodeMeta::is_deleted)
        })
        .unwrap_or(true)
}
