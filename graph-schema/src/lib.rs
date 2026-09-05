pub mod edge;
pub mod enum_property;
pub mod error;
pub mod graph;
pub mod node;
pub mod property;
pub mod schema;
pub mod storage;
pub mod strings_pool;

use std::{fmt::Debug, hash::Hash, str::FromStr};

use crate::{
    node::RawNodeId,
    property::{PropertyType, QuantityType},
};

pub trait EdgeDirectionKind:
    Sized + Copy + Clone + ItemFromIndex + ItemAsStr + Eq + Hash + Ord + 'static
{
    fn values() -> &'static [Self];
    fn factor(&self) -> usize;
    fn src_half() -> Self;
    fn dst_half() -> Self;
    fn orient_edge(&self, src: RawNodeId, dst: RawNodeId) -> (RawNodeId, Self, RawNodeId, Self);
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
    + Ord
    + Debug
    + Send
    + Sync
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
    + Ord
    + Debug
    + Send
    + Sync
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
    + Ord
    + Debug
    + Send
    + Sync
    + 'static
{
}

pub trait EnumPropertyIndex:
    ItemAsStr + ItemIndex + ItemFromIndex + ItemAll + ItemFromStr + Debug + 'static
{
    fn enum_property_index() -> usize;
}

pub trait EnumPropertyRegistry:
    ItemAsStr
    + ItemFromStr
    + ItemIndex
    + ItemFromIndex
    + ItemAll
    + Copy
    + Clone
    + Debug
    + Send
    + Sync
    + 'static
{
    fn variant_count(&self) -> usize;
}

pub trait EnumProperty:
    ItemAsStr + ItemFromStr + ItemIndex + ItemFromIndex + ItemAll + Copy + Clone + Debug + 'static
{
}
