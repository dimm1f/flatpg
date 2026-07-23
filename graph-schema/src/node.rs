use std::fmt::Display;

use crate::{
    AvailableProperties, EdgeDirectionKind, ItemAsStr, ItemIndex, ItemKindPropertyType,
    edge::{Direction, EdgeId},
    error::Error,
    graph::Graph,
    property::{PropertyValue, QuantityType},
    schema::{EdgeKind, NodeKind, PropKind, Schema},
};

pub trait StoredNode<S: Schema> {
    fn graph(&self) -> &Graph<S>;
    fn kind(&self) -> NodeKind<S>;
    fn seq(&self) -> usize;

    /// Returns an untyped, schema-erased reference to this node, suitable for
    /// passing to `Graph` lookup methods such as `get_node_property`.
    fn node_ref(&self) -> NodeRef {
        NodeRef::new(self.kind().index(), self.seq())
    }

    /// Returns this node's `edge_kind` edges for the source half of the
    /// schema's direction type (`Out` for `Direction` schemas).
    fn get_edges_out(&self, edge_kind: EdgeKind<S>) -> Result<Vec<EdgeId<S>>, Error> {
        self.graph().get_edges(
            NodeId::new(self.kind(), self.seq()),
            edge_kind,
            Direction::src_half(),
        )
    }

    /// Returns this node's `edge_kind` edges for the destination half of the
    /// schema's direction type (`In` for `Direction` schemas).
    fn get_edges_in(&self, edge_kind: EdgeKind<S>) -> Result<Vec<EdgeId<S>>, Error> {
        self.graph().get_edges(
            NodeId::new(self.kind(), self.seq()),
            edge_kind,
            Direction::dst_half(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeMeta(bool);

impl NodeMeta {
    pub(crate) fn new(is_deleted: bool) -> Self {
        Self(is_deleted)
    }

    pub fn is_deleted(&self) -> bool {
        self.0
    }

    pub(crate) fn set_is_deleted(&mut self, arg: bool) {
        self.0 = arg
    }
}

impl Default for NodeMeta {
    fn default() -> Self {
        Self::new(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef {
    kind: u32,
    seq: u32,
}

impl NodeRef {
    pub(crate) fn new(kind: usize, seq: usize) -> Self {
        assert!(kind <= u32::MAX as usize);
        assert!(seq <= u32::MAX as usize);
        Self {
            kind: kind as u32,
            seq: seq as u32,
        }
    }

    pub fn kind(&self) -> usize {
        self.kind as usize
    }

    pub fn seq(&self) -> usize {
        self.seq as usize
    }
}

impl Display for NodeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeRef({},{})", self.kind(), self.seq())
    }
}

impl<S: Schema> From<&NodeId<S>> for NodeRef {
    fn from(value: &NodeId<S>) -> Self {
        Self::new(value.kind().index(), value.seq())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId<S: Schema> {
    kind: NodeKind<S>,
    seq: usize,
}

impl<S: Schema> NodeId<S> {
    pub(crate) fn new(kind: NodeKind<S>, seq: usize) -> Self {
        Self { kind, seq }
    }

    pub fn kind(&self) -> NodeKind<S> {
        self.kind
    }

    pub fn seq(&self) -> usize {
        self.seq
    }
}

impl<S: Schema> Display for NodeId<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeId({},{})", self.kind().as_str(), self.seq())
    }
}

impl<S: Schema> TryFrom<NodeRef> for NodeId<S> {
    type Error = Error;

    fn try_from(value: NodeRef) -> Result<Self, Self::Error> {
        let kind = S::resolve_node_kind(value)?;
        Ok(Self::new(kind, value.seq()))
    }
}

pub struct NewNode<S: Schema> {
    kind: NodeKind<S>,
    properties: std::collections::HashMap<PropKind<S>, Vec<PropertyValue>>,
}

impl<S: Schema> NewNode<S> {
    pub fn new(kind: NodeKind<S>) -> Self {
        Self {
            kind,
            properties: Default::default(),
        }
    }

    pub fn add_property<T: Into<PropertyValue>>(
        &mut self,
        prop_kind: PropKind<S>,
        value: T,
    ) -> Result<(), Error> {
        if !self.kind.properties().contains(&prop_kind) {
            return Err(Error::property_not_supported(
                prop_kind.as_str(),
                self.kind.as_str(),
            ));
        }

        let props = self.properties.entry(prop_kind).or_default();

        if prop_kind.property_quantity() == QuantityType::One && !props.is_empty() {
            return Err(Error::property_already_set(prop_kind.as_str()));
        }

        props.push(value.into());

        Ok(())
    }

    pub fn kind(&self) -> NodeKind<S> {
        self.kind
    }

    pub fn properties(&self) -> &std::collections::HashMap<PropKind<S>, Vec<PropertyValue>> {
        &self.properties
    }
}
