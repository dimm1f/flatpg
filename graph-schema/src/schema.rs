use std::fmt::{Debug, Display};

use crate::edge::{Direction, EdgeHandle};
use crate::error::Error;
use crate::node::RawNodeId;
use crate::property::PropertyType;
use crate::{
    EdgeDirectionKind, EdgeItemKind, EnumPropertyRegistry, ItemAll, ItemAsStr, ItemFromIndex,
    ItemIndex, ItemKindPropertyType, NodeItemKind, PropertyItemKind,
};

/// An index into `EdgeStorage` for a given
/// `(node_kind, direction, edge_kind)` combination.
#[derive(Debug, Clone, Copy)]
pub struct EdgeStorageSlotIndex(usize);

impl EdgeStorageSlotIndex {
    /// Creates a slot handle for the given slot number.
    fn new(slot_index: usize) -> Self {
        Self(slot_index)
    }

    pub fn index(&self) -> usize {
        self.0
    }
}

impl Display for EdgeStorageSlotIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EdgeStorageSlot({})", self.0)
    }
}

/// An index into the `PropertyStorage` for a given
/// `(node_kind, property_kind)` combination.
#[derive(Debug, Clone, Copy)]
pub struct PropertyStorageSlotIndex(usize);

impl PropertyStorageSlotIndex {
    /// Creates a slot handle for the given slot number.
    fn new(slot_index: usize) -> Self {
        Self(slot_index)
    }

    pub fn index(&self) -> usize {
        self.0
    }
}

impl Display for PropertyStorageSlotIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PropertyStorageSlot({})", self.0)
    }
}

pub type NodeKind<S> = <S as Schema>::N;
pub type EdgeKind<S> = <S as Schema>::E;
pub type PropKind<S> = <S as Schema>::P;
pub type EnumPropRegistry<S> = <S as Schema>::EPR;

/// A schema's semantic version, as `(major, minor, patch)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// Builds a version from its major, minor, and patch components.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub const fn major(&self) -> u32 {
        self.major
    }

    pub const fn minor(&self) -> u32 {
        self.minor
    }

    pub const fn patch(&self) -> u32 {
        self.patch
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major(), self.minor(), self.patch())
    }
}

pub trait Schema: Sized + Clone + Copy + Debug {
    type N: NodeItemKind<Self::P>;
    type E: EdgeItemKind;
    type P: PropertyItemKind;
    type EPR: EnumPropertyRegistry;

    const NAME: &'static str;

    const VERSION: Version;

    /// Returns the schema's name.
    fn name() -> &'static str {
        Self::NAME
    }

    /// Returns the schema's version.
    fn version() -> Version {
        Self::VERSION
    }

    /// Converts a [`RawNodeId`] to its typed node kind.
    ///
    /// Returns an error if the kind index stored in the ref does not map to any known node kind.
    fn resolve_node_kind(node_ref: RawNodeId) -> Result<Self::N, Error> {
        Self::node_kind_by_index(node_ref.kind())
            .ok_or_else(|| Error::unresolved_node_kind(node_ref.kind()))
    }

    /// Converts an [`EdgeHandle`] to its typed edge kind.
    ///
    /// Reads the kind index from the handle and looks it up via [`Schema::edge_kind_by_index`].
    /// Returns an error if the index does not map to any known edge kind in this schema.
    fn resolve_edge_kind(edge_handle: EdgeHandle) -> Result<Self::E, Error> {
        Self::edge_kind_by_index(edge_handle.kind())
            .ok_or_else(|| Error::unresolved_edge_kind(edge_handle.kind()))
    }

    /// Converts an [`EdgeHandle`] to its typed edge direction.
    ///
    /// Reads the direction index from the handle and looks it up via [`Schema::direction_by_index`].
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

    /// Returns the number of registered enum kinds in the schema.
    fn number_of_enum_kinds() -> usize {
        Self::EPR::all().len()
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

    /// Returns the string name of an enum kind.
    fn enum_label(enum_kind: Self::EPR) -> &'static str {
        enum_kind.as_str()
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

    /// Returns the enum kind for the given index, or `None` if the index is out of range.
    fn enum_kind_by_index(index: usize) -> Option<Self::EPR> {
        Self::EPR::from_index(index)
    }

    /// Returns the property type carried by edges of the given kind.
    fn edge_property_type(edge_kind: Self::E) -> PropertyType {
        edge_kind.property_type()
    }

    /// Returns the property type for the given node property kind.
    fn node_property_type(node_property_kind: Self::P) -> PropertyType {
        node_property_kind.property_type()
    }

    /// Returns the number of slots in the flat edge storage array.
    ///
    /// Equals `edge_kinds * directions * node_kinds`.
    fn edge_storage_size() -> usize {
        Self::number_of_edge_kinds() * Self::number_of_node_kinds() * Direction::values().len()
    }

    /// Returns the storage slot for the given `(node_kind, direction, edge_kind)` combination.
    ///
    /// The flat array is laid out with edge kind as the outermost dimension, direction in the
    /// middle, and node kind as the innermost, so adjacent node kinds share a cache line.
    fn edge_storage_slot(
        node_kind: Self::N,
        direction: Direction,
        edge_kind: Self::E,
    ) -> EdgeStorageSlotIndex {
        EdgeStorageSlotIndex::new(
            node_kind.index()
                + Self::number_of_node_kinds()
                    * (direction.factor() + Direction::values().len() * edge_kind.index()),
        )
    }

    /// Iterates over all `(node_kind, direction, edge_kind)` combinations in flat array order.
    ///
    /// The order matches the layout used by [`Schema::edge_storage_slot`]: edge kind outermost,
    /// direction in the middle, node kind innermost. Each item corresponds to one
    fn edge_storage_slots_iter() -> impl Iterator<Item = (Self::N, Direction, Self::E)> {
        Self::edge_kinds().iter().flat_map(|&edge_kind| {
            Direction::values().iter().flat_map(move |&direction| {
                Self::node_kinds()
                    .iter()
                    .map(move |&node_kind| (node_kind, direction, edge_kind))
            })
        })
    }

    /// Returns the number of slots in the flat property storage array.
    ///
    /// Equals `node_kinds * property_kinds`.
    fn property_storage_size() -> usize {
        Self::number_of_node_kinds() * Self::number_of_property_kinds()
    }

    /// Returns the storage slot for the given `(node_kind, property_kind)` combination.
    ///
    /// The flat array is laid out with property kind as the outermost dimension and node kind
    /// as the innermost.
    fn property_storage_slot(
        node_kind: Self::N,
        property_kind: Self::P,
    ) -> PropertyStorageSlotIndex {
        PropertyStorageSlotIndex::new(
            node_kind.index() + Self::number_of_node_kinds() * property_kind.index(),
        )
    }

    /// Iterates over all `(node_kind, property_kind)` combinations in flat array order.
    ///
    /// The order matches the layout used by [`Schema::property_storage_slot`]: property kind
    /// outermost, node kind innermost. Each item corresponds to one
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

    /// Returns all registered enum kinds in the schema.
    fn enum_kinds() -> &'static [Self::EPR] {
        Self::EPR::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering_follows_major_then_minor_then_patch_precedence() {
        assert!(Version::new(1, 0, 0) < Version::new(1, 1, 0));
        assert!(Version::new(1, 9, 9) < Version::new(2, 0, 0));
        assert!(Version::new(1, 0, 0) < Version::new(1, 0, 1));
        assert!(Version::new(1, 2, 3) == Version::new(1, 2, 3));
    }

    #[test]
    fn version_display_formats_as_dotted_triple() {
        assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
    }
}
