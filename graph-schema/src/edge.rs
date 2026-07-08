use crate::{
    EdgeDirectionKind, ItemAsStr, ItemFromIndex, ItemIndex,
    error::Error,
    node::{Node, NodeRef},
    schema::{EdgeKind, Schema},
};

/// Direction of a half-edge, for schemas whose edges have a meaningful
/// orientation.
///
/// Every edge is stored as two halves, one on each endpoint, so that either
/// node can look up its incident edges without scanning the whole graph.
/// `Direction` labels which half a given half-edge is. Use this as a
/// schema's [`Schema::D`](crate::schema::Schema::D) type when edges have a
/// direction (e.g. "follows", "parent of").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    fn make_edge(src: NodeRef, dst: NodeRef, direction: Self, handle: EdgeHandle) -> EdgeRef {
        match direction {
            Direction::In => EdgeRef {
                src_node_ref: dst,
                dst_node_ref: src,
                handle,
            },
            Direction::Out => EdgeRef {
                src_node_ref: src,
                dst_node_ref: dst,
                handle,
            },
        }
    }

    fn src_half() -> Self {
        Self::Out
    }

    fn dst_half() -> Self {
        Self::In
    }

    fn orient_edge(&self, src: NodeRef, dst: NodeRef) -> (NodeRef, Self, NodeRef, Self) {
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

#[derive(Debug, Clone, Copy)]
pub struct EdgeHandle {
    kind: u32,
    direction: u32,
    seq: u32,
}

impl EdgeHandle {
    pub(crate) fn new(kind: usize, direction: usize, seq: usize) -> Self {
        assert!(kind <= u32::MAX as usize);
        assert!(direction <= u32::MAX as usize);
        assert!(seq <= u32::MAX as usize);

        Self {
            kind: kind as u32,
            direction: direction as u32,
            seq: seq as u32,
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
}

impl<S: Schema> From<&Edge<S>> for EdgeHandle {
    fn from(value: &Edge<S>) -> Self {
        Self::new(
            value.kind().index(),
            value.direction().factor(),
            value.seq(),
        )
    }
}

pub struct EdgeRef {
    src_node_ref: NodeRef,
    dst_node_ref: NodeRef,
    handle: EdgeHandle,
}

impl EdgeRef {
    pub(crate) fn new(src_node_ref: NodeRef, dst_node_ref: NodeRef, handle: EdgeHandle) -> Self {
        Self {
            src_node_ref,
            dst_node_ref,
            handle,
        }
    }
    pub fn src_node_ref(&self) -> NodeRef {
        self.src_node_ref
    }

    pub fn dst(&self) -> NodeRef {
        self.dst_node_ref
    }

    pub fn handle(&self) -> EdgeHandle {
        self.handle
    }
}

impl<S: Schema> From<&Edge<S>> for EdgeRef {
    fn from(value: &Edge<S>) -> Self {
        let handle = EdgeHandle::from(value);
        Self::new(
            (&value.src_node()).into(),
            (&value.dst_node()).into(),
            handle,
        )
    }
}

pub struct Edge<S: Schema> {
    src_node: Node<S>,
    dst_node: Node<S>,
    kind: EdgeKind<S>,
    direction: Direction,
    seq: usize,
}

impl<S: Schema> Edge<S> {
    pub(crate) fn new(
        src_node: Node<S>,
        dst_node: Node<S>,
        kind: EdgeKind<S>,
        direction: Direction,
        seq: usize,
    ) -> Self {
        Self {
            src_node,
            dst_node,
            kind,
            direction,
            seq,
        }
    }

    pub fn src_node(&self) -> Node<S> {
        self.src_node
    }

    pub fn dst_node(&self) -> Node<S> {
        self.dst_node
    }

    pub fn kind(&self) -> EdgeKind<S> {
        self.kind
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn seq(&self) -> usize {
        self.seq
    }
}

impl<S: Schema> TryFrom<EdgeRef> for Edge<S> {
    type Error = Error;

    fn try_from(value: EdgeRef) -> Result<Self, Self::Error> {
        let src_node = value.src_node_ref.try_into()?;
        let dst_node = value.dst_node_ref.try_into()?;
        let kind = S::resolve_edge_kind(value.handle())?;
        let direction = S::resolve_edge_direction(value.handle())?;

        Ok(Self::new(
            src_node,
            dst_node,
            kind,
            direction,
            value.handle().seq(),
        ))
    }
}
