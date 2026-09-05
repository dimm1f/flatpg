use crate::{
    edge::{Direction, EdgeId, RawEdgeId},
    error::Error,
    graph::{Graph, GraphView},
    node::{NewNode, NodeId, RawNodeId},
    property::PropertyValue,
    schema::{EdgeKind, PropKind, Schema},
    storage::StoredProperty,
};

mod convert;
mod prepare;
mod slots;
mod staged;

pub use staged::StagedDiff;

type NewEdgeId = usize;

struct NewEdge<S: Schema> {
    src: NewOrExistingNode,
    dst: NewOrExistingNode,
    kind: EdgeKind<S>,
    property: Option<PropertyValue>,
}

struct HalfEdge<S: Schema> {
    node: NodeId<S>,
    neighbor: RawNodeId,
    direction: Direction,
    edge_kind: EdgeKind<S>,
    property: Option<StoredProperty>,
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
    // after another. Ids from an earlier diff can become wrong once an earlier `apply` or
    // `StagedDiff::commit` call changes the graph, and this does not always cause an error:
    // - `remove_edge` finds a half-edge by its position in the node's own edge list. When one
    //   diff removes an edge, every later edge on that node moves one position down. If another
    //   diff still holds an edge id from before that removal, it may now point to a different
    //   edge on the same node and remove it silently, with no error.
    // - `remove_node` only marks a node as deleted, so node ids stay valid across diffs. But
    //   `update_node_property` does not check whether the node was already deleted by an
    //   earlier diff, so a later diff can still write a property onto a deleted node.
    // To stay safe, apply one diff, then build the next diff from the graph left by that call,
    // instead of reusing ids from before it.
    // Planned: graph versioning removes this hazard. A `Graph` will carry a version, a
    // `GraphDiff` will be derived from a graph and pinned to the version it saw when it was
    // created, and `prepare` will reject a diff whose pinned version no longer matches the
    // graph instead of silently resolving stale ids against it. A `GraphDiff` created without
    // a graph to derive from will then only be applicable to an empty graph.
    pub fn apply(self, graph: impl GraphView<S>) -> Result<(Graph<S>, Vec<NodeId<S>>), Error> {
        let mut graph = graph.into_graph();
        let node_remapper = self.prepare(&mut graph)?.commit();
        Ok((graph, node_remapper))
    }
}
