use std::result;

use thiserror::Error;

use crate::property::PropertyType;

pub type Result<T> = result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid property type (expected \"{expected}\", found \"{found}\")")]
    InvalidPropertyType { expected: String, found: String },
    #[error("invalid slot index \"{0}\"")]
    InvalidSlotIndex(String),
    #[error("property index is not found")]
    PropertyIndexNotFound,
    #[error("property \"{property}\" is not supported for node \"{node_kind_name}\"")]
    PropertyNotSupported {
        property: String,
        node_kind_name: String,
    },
    #[error("property \"{0}\" with quantity \"One\" is already set")]
    PropertyAlreadySet(String),
    #[error(
        "property indices range \"{start} - {end}\" is out of bounds, properties count: {count}"
    )]
    PropertyIndexOutOfBounds {
        start: usize,
        end: usize,
        count: usize,
    },
    #[error("offsets are not found for slot \"{0}\"")]
    SlotOffsetsNotFound(String),
    #[error("offsets are not found for node.seq \"{0}\"")]
    NodeOffsetNotFound(usize),
    #[error("end index less than start")]
    InvalidEdgeRange,
    #[error("node {node} has more \"{edge_kind} {direction}\" edges than EdgeSeq allows")]
    TooManyEdges {
        node: String,
        edge_kind: String,
        direction: String,
    },
    #[error(
        "neighbors are not found for index: 
    
    {0}"
    )]
    NeighborNotFound(usize),
    #[error("failed to resolve node kind {0}")]
    UnresolvedNodeKind(usize),
    #[error("failed to resolve edge kind {0}")]
    UnresolvedEdgeKind(usize),
    #[error("failed to resolve direction {0}")]
    UnresolvedDirection(usize),
    #[error("unknown {enum_name} label: \"{label}\"")]
    UnknownLabel { enum_name: String, label: String },
    #[error("reverse edge not found: {target} in node {node}'s {direction} {edge_kind} list")]
    ReverseEdgeNotFound {
        target: String,
        node: String,
        direction: String,
        edge_kind: String,
    },
    #[error("failed to resolve string ref \"{0}\"")]
    UnresolvedStringRef(String),
}

impl Error {
    pub fn invalid_property_type(expected: PropertyType, found: PropertyType) -> Self {
        Self::InvalidPropertyType {
            expected: expected.to_string(),
            found: found.to_string(),
        }
    }

    pub fn invalid_slot_index(slot: impl Into<String>) -> Self {
        Self::InvalidSlotIndex(slot.into())
    }

    pub fn property_index_not_found() -> Self {
        Self::PropertyIndexNotFound
    }

    pub fn property_not_supported(
        property: impl Into<String>,
        node_kind_name: impl Into<String>,
    ) -> Self {
        Self::PropertyNotSupported {
            property: property.into(),
            node_kind_name: node_kind_name.into(),
        }
    }

    pub fn property_already_set(property: impl Into<String>) -> Self {
        Self::PropertyAlreadySet(property.into())
    }

    pub fn property_index_out_of_bounds(start: usize, end: usize, count: usize) -> Self {
        Self::PropertyIndexOutOfBounds { start, end, count }
    }

    pub fn slot_offsets_not_found(slot: impl Into<String>) -> Self {
        Self::SlotOffsetsNotFound(slot.into())
    }

    pub fn node_offset_not_found(seq: usize) -> Self {
        Self::NodeOffsetNotFound(seq)
    }

    pub fn invalid_edge_range() -> Self {
        Self::InvalidEdgeRange
    }

    pub fn too_many_edges(
        node: impl Into<String>,
        edge_kind: impl Into<String>,
        direction: impl Into<String>,
    ) -> Self {
        Self::TooManyEdges {
            node: node.into(),
            edge_kind: edge_kind.into(),
            direction: direction.into(),
        }
    }

    pub fn neighbor_not_found(index: usize) -> Self {
        Self::NeighborNotFound(index)
    }

    pub fn unresolved_node_kind(kind_index: usize) -> Self {
        Self::UnresolvedNodeKind(kind_index)
    }

    pub fn unresolved_edge_kind(kind_index: usize) -> Self {
        Self::UnresolvedEdgeKind(kind_index)
    }

    pub fn unresolved_direction(factor: usize) -> Self {
        Self::UnresolvedDirection(factor)
    }

    pub fn unknown_label(enum_name: impl Into<String>, label: impl Into<String>) -> Self {
        Self::UnknownLabel {
            enum_name: enum_name.into(),
            label: label.into(),
        }
    }

    pub fn reverse_edge_not_found(
        target: impl Into<String>,
        node: impl Into<String>,
        direction: impl Into<String>,
        edge_kind: impl Into<String>,
    ) -> Self {
        Self::ReverseEdgeNotFound {
            target: target.into(),
            node: node.into(),
            direction: direction.into(),
            edge_kind: edge_kind.into(),
        }
    }

    pub fn unresolved_string_ref(str_ref: impl Into<String>) -> Self {
        Self::UnresolvedStringRef(str_ref.into())
    }
}
