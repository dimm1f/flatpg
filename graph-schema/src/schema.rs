use std::fmt::Display;

use crate::edge::{Direction, EdgeHandle, EdgeRef};
use crate::error::Error;
use crate::node::NodeRef;
use crate::property::PropertyType;
use crate::{
    EdgeDirectionKind, EdgeItemKind, ItemAll, ItemAsStr, ItemFromIndex, ItemIndex,
    ItemKindPropertyType, NodeItemKind, PropertyItemKind,
};

// Constants
const NEIGHBORS_SLOT_SIZE: usize = 3;
const PROPERTY_SLOT_SIZE: usize = 2;

/// A handle to a group of consecutive entries in the flat edge storage array.
///
/// Each slot covers [`EdgeStorageSlot::size`] entries: one for offsets, one for
/// neighbors, and one for edge properties.
#[derive(Debug, Clone, Copy)]
pub struct EdgeStorageSlot(usize);

impl EdgeStorageSlot {
    /// Creates a slot handle for the given slot number.
    fn new(slot_index: usize) -> Self {
        Self(slot_index * Self::size())
    }

    /// Returns the number of array entries per slot (currently 3).
    pub const fn size() -> usize {
        NEIGHBORS_SLOT_SIZE
    }

    /// Returns the array index of the offsets entry for this slot.
    #[inline]
    pub fn offset_index(&self) -> usize {
        self.0
    }

    /// Returns the array index of the neighbors entry for this slot.
    #[inline]
    pub fn neighbors_index(&self) -> usize {
        self.0 + 1
    }

    /// Returns the array index of the edge properties entry for this slot.
    #[inline]
    pub fn properties_index(&self) -> usize {
        self.0 + 2
    }
}

impl Display for EdgeStorageSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EdgeStorageSlot({})", self.0)
    }
}

/// A handle to a group of consecutive entries in the flat property storage array.
///
/// Each slot covers [`PropertyStorageSlot::size`] entries: one for offsets and one for values.
#[derive(Debug, Clone, Copy)]
pub struct PropertyStorageSlot(usize);

impl PropertyStorageSlot {
    /// Creates a slot handle for the given slot number.
    fn new(slot_index: usize) -> Self {
        Self(slot_index * Self::size())
    }

    /// Returns the number of array entries per slot (currently 2).
    pub const fn size() -> usize {
        PROPERTY_SLOT_SIZE
    }

    /// Returns the array index of the offsets entry for this slot.
    #[inline]
    pub fn offset_index(&self) -> usize {
        self.0
    }

    /// Returns the array index of the values entry for this slot.
    #[inline]
    pub fn values_index(&self) -> usize {
        self.0 + 1
    }
}

impl Display for PropertyStorageSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PropertyStorageSlot({})", self.0)
    }
}

pub type NodeKind<S> = <S as Schema>::N;
pub type EdgeKind<S> = <S as Schema>::E;
pub type PropKind<S> = <S as Schema>::P;

pub trait Schema: Sized + Clone + Copy {
    type N: NodeItemKind<Self::P>;
    type E: EdgeItemKind;
    type P: PropertyItemKind;

    /// Builds an [`Edge`] from a source node, a neighbor node, a direction, and an edge ref.
    fn make_edge(
        src_node_ref: NodeRef,
        dst_node_ref: NodeRef,
        direction: Direction,
        edge_handle: EdgeHandle,
    ) -> EdgeRef {
        Direction::make_edge(src_node_ref, dst_node_ref, direction, edge_handle)
    }

    /// Converts a [`NodeRef`] to its typed node kind.
    ///
    /// Returns an error if the kind index stored in the ref does not map to any known node kind.
    fn resolve_node_kind(node_ref: NodeRef) -> Result<Self::N, Error> {
        Self::node_kind_by_index(node_ref.kind())
            .ok_or_else(|| Error::unresolved_node_kind(node_ref.kind()))
    }

    /// Converts an [`EdgeRef`] to its typed edge kind.
    ///
    /// Reads the kind index from the ref and looks it up via [`Schema::edge_kind_by_index`].
    /// Returns an error if the index does not map to any known edge kind in this schema.
    fn resolve_edge_kind(edge_handle: EdgeHandle) -> Result<Self::E, Error> {
        Self::edge_kind_by_index(edge_handle.kind())
            .ok_or_else(|| Error::unresolved_edge_kind(edge_handle.kind()))
    }

    /// Converts an [`EdgeRef`] to its typed edge direction.
    ///
    /// Reads the direction index from the ref and looks it up via [`Schema::direction_by_index`].
    /// Returns an error if the index does not map to any known direction in this schema.
    fn resolve_edge_direction(edge_handle: EdgeHandle) -> Result<Direction, Error> {
        Self::direction_by_index(edge_handle.direction())
            .ok_or_else(|| Error::unresolved_direction(edge_handle.direction()))
    }

    /// Returns the number of node kinds in the schema.
    fn number_of_node_kinds() -> usize {
        Self::N::all().len()
    }

    /// Returns the number of edge kinds in the schema.
    fn number_of_edge_kinds() -> usize {
        Self::E::all().len()
    }

    /// Returns the number of property kinds in the schema.
    fn number_of_property_kinds() -> usize {
        Self::P::all().len()
    }

    /// Returns the string name of a node kind.
    fn node_label(node_kind: Self::N) -> &'static str {
        node_kind.as_str()
    }

    /// Returns the string name of an edge kind.
    fn edge_label(edge_kind: Self::E) -> &'static str {
        edge_kind.as_str()
    }

    /// Returns the string name of a property kind.
    fn property_label(property_kind: Self::P) -> &'static str {
        property_kind.as_str()
    }

    /// Returns the node kind for the given index, or `None` if the index is out of range.
    fn node_kind_by_index(index: usize) -> Option<Self::N> {
        Self::N::from_index(index)
    }

    /// Returns the edge kind for the given index, or `None` if the index is out of range.
    fn edge_kind_by_index(index: usize) -> Option<Self::E> {
        Self::E::from_index(index)
    }

    /// Returns the direction for the given index, or `None` if the index is out of range.
    fn direction_by_index(index: usize) -> Option<Direction> {
        Direction::from_index(index)
    }

    /// Returns the property kind for the given index, or `None` if the index is out of range.
    fn property_kind_by_index(index: usize) -> Option<Self::P> {
        Self::P::from_index(index)
    }

    /// Returns the property type carried by edges of the given kind.
    fn edge_property_type(edge_kind: Self::E) -> PropertyType {
        edge_kind.property_type()
    }

    /// Returns the property type for the given node property kind.
    fn node_property_type(node_property_kind: Self::P) -> PropertyType {
        node_property_kind.property_type()
    }

    /// Returns the total number of entries in the flat edge storage array.
    ///
    /// Equals `edge_kinds * directions * node_kinds * EdgeStorageSlot::size()`.
    fn edge_storage_size() -> usize {
        Self::number_of_edge_kinds()
            * Self::number_of_node_kinds()
            * EdgeStorageSlot::size()
            * Direction::values().len()
    }

    /// Returns the storage slot for the given `(node_kind, direction, edge_kind)` combination.
    ///
    /// The flat array is laid out with edge kind as the outermost dimension, direction in the
    /// middle, and node kind as the innermost, so adjacent node kinds share a cache line.
    fn edge_storage_slot(
        node_kind: Self::N,
        direction: Direction,
        edge_kind: Self::E,
    ) -> EdgeStorageSlot {
        EdgeStorageSlot::new(
            node_kind.index()
                + Self::number_of_node_kinds()
                    * (direction.factor() + Direction::values().len() * edge_kind.index()),
        )
    }

    /// Iterates over all `(node_kind, direction, edge_kind)` combinations in flat array order.
    ///
    /// The order matches the layout used by [`Schema::edge_storage_slot`]: edge kind outermost,
    /// direction in the middle, node kind innermost. Each item corresponds to one [`EdgeStorageSlot`].
    fn edge_storage_slots_iter() -> impl Iterator<Item = (Self::N, Direction, Self::E)> {
        Self::edge_kinds().iter().flat_map(|&edge_kind| {
            Direction::values().iter().flat_map(move |&direction| {
                Self::node_kinds()
                    .iter()
                    .map(move |&node_kind| (node_kind, direction, edge_kind))
            })
        })
    }

    /// Returns the total number of entries in the flat property storage array.
    ///
    /// Equals `node_kinds * property_kinds * PropertyStorageSlot::size()`.
    fn property_storage_size() -> usize {
        Self::number_of_node_kinds()
            * Self::number_of_property_kinds()
            * PropertyStorageSlot::size()
    }

    /// Returns the storage slot for the given `(node_kind, property_kind)` combination.
    ///
    /// The flat array is laid out with property kind as the outermost dimension and node kind
    /// as the innermost.
    fn property_storage_slot(node_kind: Self::N, property_kind: Self::P) -> PropertyStorageSlot {
        PropertyStorageSlot::new(
            node_kind.index() + Self::number_of_node_kinds() * property_kind.index(),
        )
    }

    /// Iterates over all `(node_kind, property_kind)` combinations in flat array order.
    ///
    /// The order matches the layout used by [`Schema::property_storage_slot`]: property kind
    /// outermost, node kind innermost. Each item corresponds to one [`PropertyStorageSlot`].
    fn property_storage_slots_iter() -> impl Iterator<Item = (Self::N, Self::P)> {
        Self::property_kinds().iter().flat_map(|&property_kind| {
            Self::node_kinds()
                .iter()
                .map(move |&node_kind| (node_kind, property_kind))
        })
    }

    /// Returns all node kinds in the schema.
    fn node_kinds() -> &'static [Self::N] {
        Self::N::all()
    }

    /// Returns all edge kinds in the schema.
    fn edge_kinds() -> &'static [Self::E] {
        Self::E::all()
    }

    /// Returns all property kinds in the schema.
    fn property_kinds() -> &'static [Self::P] {
        Self::P::all()
    }
}
