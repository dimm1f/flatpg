pub mod edge;
pub mod error;
pub mod graph;
pub mod node;
pub mod property;
pub mod schema;
pub mod storage;
pub mod strings_pool;

use std::{hash::Hash, str::FromStr};

use crate::{
    edge::{EdgeHandle, EdgeRef},
    node::NodeRef,
    property::{PropertyType, QuantityType},
};

pub trait EdgeDirectionKind:
    Sized + Copy + Clone + ItemFromIndex + ItemAsStr + Eq + Hash + 'static
{
    fn values() -> &'static [Self];
    fn factor(&self) -> usize;
    fn make_edge(
        src_node_ref: NodeRef,
        dst_node_ref: NodeRef,
        direction: Self,
        edge_handle: EdgeHandle,
    ) -> EdgeRef;
    fn src_half() -> Self;
    fn dst_half() -> Self;
    fn orient_edge(&self, src: NodeRef, dst: NodeRef) -> (NodeRef, Self, NodeRef, Self);
}

pub trait ItemAsStr {
    fn as_str(&self) -> &'static str;
}
pub trait ItemIndex {
    fn index(&self) -> usize;
}

pub trait ItemFromIndex: Sized {
    fn from_index(index: usize) -> Option<Self>;
}

pub trait ItemFromStr: FromStr {}

pub trait ItemKindPropertyType {
    type PropertyType;
    type QuantityType;

    fn property_type(&self) -> PropertyType;
    fn property_quantity(&self) -> QuantityType;
}

pub trait ItemAll: Sized {
    fn all() -> &'static [Self];
}

pub trait AvailableProperties<P>
where
    P: PropertyItemKind,
{
    fn properties(&self) -> &'static [P];
}

pub trait GraphItemKind:
    ItemAsStr + ItemIndex + ItemFromIndex + ItemAll + ItemFromStr + 'static
{
}

impl<T> GraphItemKind for T where
    T: ItemAsStr + ItemIndex + ItemFromIndex + ItemAll + ItemFromStr + 'static
{
}

pub trait NodeItemKind<P: PropertyItemKind>:
    ItemAsStr
    + ItemFromStr
    + ItemIndex
    + ItemFromIndex
    + ItemAll
    + AvailableProperties<P>
    + Copy
    + Clone
    + Eq
    + Hash
    + 'static
{
}

pub trait EdgeItemKind:
    ItemAsStr
    + ItemFromStr
    + ItemIndex
    + ItemFromIndex
    + ItemAll
    + ItemKindPropertyType
    + Copy
    + Clone
    + Eq
    + Hash
    + 'static
{
}

pub trait PropertyItemKind:
    ItemAsStr
    + ItemFromStr
    + ItemIndex
    + ItemFromIndex
    + ItemAll
    + ItemKindPropertyType
    + Copy
    + Clone
    + Eq
    + Hash
    + 'static
{
}
