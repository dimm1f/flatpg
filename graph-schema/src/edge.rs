use crate::{
    EdgeDirectionKind, ItemAsStr, ItemFromIndex, ItemIndex,
    error::Error,
    graph::Graph,
    node::{NodeId, RawNodeId},
    schema::{EdgeKind, Schema},
    storage::EdgeSeq,
};

pub trait StoredEdge<S: Schema> {
    fn graph(&self) -> &Graph<S>;
    fn kind(&self) -> EdgeKind<S>;
    fn src_node(&self) -> NodeId<S>;
    fn dst_node(&self) -> NodeId<S>;
    fn direction(&self) -> Direction;
    fn seq(&self) -> usize;

    /// The identity of the half-edge this names, read back from the graph.
    ///
    /// `None` when the edge's kind carries no property and so allocates no identity, and also
    /// when the position no longer resolves — in which case the id is stale either way, and
    /// whatever it is passed to reports that itself.
    fn edge_seq(&self) -> Option<EdgeSeq> {
        self.graph().half_edge_seq(
            (&self.src_node()).into(),
            (&self.dst_node()).into(),
            self.kind(),
            self.direction(),
            self.seq(),
        )
    }

    fn edge(&self) -> EdgeId<S> {
        EdgeId::new(
            self.src_node(),
            self.dst_node(),
            self.kind(),
            self.direction(),
            self.seq(),
            self.edge_seq(),
        )
    }
}

/// Direction of a half-edge.
///
/// Every edge is stored as two halves, one on each endpoint, so that either
/// node can look up its incident edges without scanning the whole graph.
/// `Direction` labels which half a given half-edge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    /// The half stored on the edge's destination node, pointing back at the source.
    In,
    /// The half stored on the edge's source node, pointing at the destination.
    Out,
}

impl EdgeDirectionKind for Direction {
    fn values() -> &'static [Direction] {
        const ARRAY: [Direction; 2] = [Direction::In, Direction::Out];
        &ARRAY
    }

    fn factor(&self) -> usize {
        match self {
            Self::In => 0,
            Self::Out => 1,
        }
    }

    fn src_half() -> Self {
        Self::Out
    }

    fn dst_half() -> Self {
        Self::In
    }

    fn orient_edge(&self, src: RawNodeId, dst: RawNodeId) -> (RawNodeId, Self, RawNodeId, Self) {
        match *self {
            Self::Out => (src, Self::Out, dst, Self::In),
            Self::In => (dst, Self::In, src, Self::Out),
        }
    }
}

impl ItemFromIndex for Direction {
    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::In),
            1 => Some(Self::Out),
            _ => None,
        }
    }
}

impl ItemAsStr for Direction {
    fn as_str(&self) -> &'static str {
        match self {
            Direction::In => "In",
            Direction::Out => "Out",
        }
    }
}

/// Stands in for an absent [`EdgeSeq`] in [`EdgeHandle`], which keeps its fields as plain
/// `u32`s. Sound because `EdgeSeq::new` rejects a count that does not fit in a `u32`, so
/// `u32::MAX` can never name a real edge.
const NO_EDGE_SEQ: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
pub struct EdgeHandle {
    kind: u32,
    direction: u32,
    seq: u32,
    edge_seq: u32,
}

impl EdgeHandle {
    pub(crate) fn new(
        kind: usize,
        direction: usize,
        seq: usize,
        edge_seq: Option<EdgeSeq>,
    ) -> Self {
        assert!(kind <= u32::MAX as usize);
        assert!(direction <= u32::MAX as usize);
        assert!(seq <= u32::MAX as usize);

        Self {
            kind: kind as u32,
            direction: direction as u32,
            seq: seq as u32,
            edge_seq: edge_seq.map_or(NO_EDGE_SEQ, |seq| seq.index() as u32),
        }
    }
    pub fn kind(&self) -> usize {
        self.kind as usize
    }

    pub fn direction(&self) -> usize {
        self.direction as usize
    }

    pub fn seq(&self) -> usize {
        self.seq as usize
    }

    /// The edge's identity, or `None` for a kind that carries no property and so allocates
    /// none. See [`EdgeId::edge_seq`].
    pub fn edge_seq(&self) -> Option<EdgeSeq> {
        (self.edge_seq != NO_EDGE_SEQ).then(|| {
            // In range by construction: the value came from an `EdgeSeq`, which is a `u32`.
            EdgeSeq::new(self.edge_seq as usize).expect("round-trips a u32-backed EdgeSeq")
        })
    }
}

impl<S: Schema> From<&EdgeId<S>> for EdgeHandle {
    fn from(value: &EdgeId<S>) -> Self {
        Self::new(
            value.kind().index(),
            value.direction().factor(),
            value.seq(),
            value.edge_seq(),
        )
    }
}

pub struct RawEdgeId {
    src_node_id: RawNodeId,
    dst_node_id: RawNodeId,
    handle: EdgeHandle,
}

impl RawEdgeId {
    pub fn new(src_node_id: RawNodeId, dst_node_id: RawNodeId, handle: EdgeHandle) -> Self {
        Self {
            src_node_id,
            dst_node_id,
            handle,
        }
    }
    pub fn src_node_id(&self) -> RawNodeId {
        self.src_node_id
    }

    pub fn dst(&self) -> RawNodeId {
        self.dst_node_id
    }

    pub fn handle(&self) -> EdgeHandle {
        self.handle
    }
}

impl<S: Schema> From<&EdgeId<S>> for RawEdgeId {
    fn from(value: &EdgeId<S>) -> Self {
        let handle = EdgeHandle::from(value);
        Self::new(
            (&value.src_node()).into(),
            (&value.dst_node()).into(),
            handle,
        )
    }
}

pub struct EdgeId<S: Schema> {
    src_node: NodeId<S>,
    dst_node: NodeId<S>,
    kind: EdgeKind<S>,
    direction: Direction,
    seq: usize,
    edge_seq: Option<EdgeSeq>,
}

impl<S: Schema> EdgeId<S> {
    pub(crate) fn new(
        src_node: NodeId<S>,
        dst_node: NodeId<S>,
        kind: EdgeKind<S>,
        direction: Direction,
        seq: usize,
        edge_seq: Option<EdgeSeq>,
    ) -> Self {
        Self {
            src_node,
            dst_node,
            kind,
            direction,
            seq,
            edge_seq,
        }
    }

    /// Builds a half-edge from the two nodes it joins.
    ///
    /// `near` is the node whose adjacency list the half-edge was read from and `far` the
    /// neighbor stored there; `direction` decides which of the two becomes `src_node`. This
    /// is the inverse of [`EdgeDirectionKind::orient_edge`].
    pub(crate) fn from_half(
        near: NodeId<S>,
        far: NodeId<S>,
        kind: EdgeKind<S>,
        direction: Direction,
        seq: usize,
        edge_seq: Option<EdgeSeq>,
    ) -> Self {
        let (src_node, dst_node) = match direction {
            Direction::Out => (near, far),
            Direction::In => (far, near),
        };

        Self::new(src_node, dst_node, kind, direction, seq, edge_seq)
    }

    pub fn src_node(&self) -> NodeId<S> {
        self.src_node
    }

    pub fn dst_node(&self) -> NodeId<S> {
        self.dst_node
    }

    pub fn kind(&self) -> EdgeKind<S> {
        self.kind
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// The edge's position in the adjacency list it was read from.
    ///
    /// A position, unlike [`EdgeId::edge_seq`], is not stable: removing an earlier edge on the
    /// same node shifts every later one down.
    pub fn seq(&self) -> usize {
        self.seq
    }

    /// The edge's identity within its kind, or `None` for a kind carrying no property.
    ///
    /// Both halves of an edge share this, and it survives other edges being removed, so it is
    /// what [`GraphDiff::remove_edge`](crate::graph::builder::GraphDiff::remove_edge) resolves
    /// an edge by. [`Graph::compact_edge_properties`](crate::graph::Graph::compact_edge_properties)
    /// renumbers it, and so invalidates ids taken before it ran.
    pub fn edge_seq(&self) -> Option<EdgeSeq> {
        self.edge_seq
    }
}

impl<S: Schema> TryFrom<RawEdgeId> for EdgeId<S> {
    type Error = Error;

    fn try_from(value: RawEdgeId) -> Result<Self, Self::Error> {
        let src_node = value.src_node_id.try_into()?;
        let dst_node = value.dst_node_id.try_into()?;
        let kind = S::resolve_edge_kind(value.handle())?;
        let direction = S::resolve_edge_direction(value.handle())?;

        Ok(Self::new(
            src_node,
            dst_node,
            kind,
            direction,
            value.handle().seq(),
            value.handle().edge_seq(),
        ))
    }
}
