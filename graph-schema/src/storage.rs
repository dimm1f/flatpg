use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{
    enum_property::RawEnumId,
    error::Error,
    node::{NodeMeta, RawNodeId},
    property::PropertyType,
    schema::Schema,
    strings_pool::RawStringId,
};

pub(crate) fn ranged_slice<T>(v: &[T], range: std::ops::Range<usize>) -> Result<&[T], Error> {
    let (start, end) = (range.start, range.end);
    v.get(range)
        .ok_or_else(|| Error::property_index_out_of_bounds(start, end, v.len()))
}

#[derive(Debug, Clone)]
pub enum StoredProperty {
    Bool(bool),
    Byte(u8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    NodeId(RawNodeId),
    StringId(RawStringId),
    Enum(RawEnumId),
}

impl StoredProperty {
    pub fn typ(&self) -> PropertyType {
        match self {
            StoredProperty::Bool(_) => PropertyType::Bool,
            StoredProperty::Byte(_) => PropertyType::Byte,
            StoredProperty::Short(_) => PropertyType::Short,
            StoredProperty::Int(_) => PropertyType::Int,
            StoredProperty::Long(_) => PropertyType::Long,
            StoredProperty::Float(_) => PropertyType::Float,
            StoredProperty::Double(_) => PropertyType::Double,
            StoredProperty::NodeId(_) => PropertyType::NodeId,
            StoredProperty::StringId(_) => PropertyType::String,
            StoredProperty::Enum(_) => PropertyType::Enum,
        }
    }
}

type InnerOffset = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset(InnerOffset);

impl Offset {
    pub fn new(value: usize) -> Result<Self, Error> {
        InnerOffset::try_from(value)
            .map(Self)
            .map_err(|_| Error::offset_overflow(value))
    }

    #[inline]
    pub fn zero() -> Self {
        Self(0)
    }

    #[inline]
    pub fn value(&self) -> usize {
        self.0 as usize
    }

    /// `self - rhs`, as a plain length. Fails if `rhs > self` (offsets not non-decreasing).
    pub fn checked_sub(self, rhs: Self) -> Result<usize, Error> {
        self.0
            .checked_sub(rhs.0)
            .map(|v| v as usize)
            .ok_or_else(Error::offset_underflow)
    }

    /// `self + delta`. Fails if the result (or `delta` itself) doesn't fit in `InnerOffset`.
    pub fn checked_add_delta(self, delta: usize) -> Result<Self, Error> {
        let delta = InnerOffset::try_from(delta).map_err(|_| Error::offset_overflow(delta))?;
        self.0
            .checked_add(delta)
            .map(Self)
            .ok_or_else(|| Error::offset_overflow(self.value().saturating_add(delta as usize)))
    }

    /// `self - delta`. Fails if `delta > self` (offsets not non-decreasing).
    pub fn checked_sub_delta(self, delta: usize) -> Result<Self, Error> {
        let delta = InnerOffset::try_from(delta).map_err(|_| Error::offset_underflow())?;
        self.0
            .checked_sub(delta)
            .map(Self)
            .ok_or_else(Error::offset_underflow)
    }
}

impl From<&Offset> for usize {
    fn from(value: &Offset) -> Self {
        value.value()
    }
}

#[derive(Debug, Clone, Default)]
pub enum StorageArray {
    Bool(Vec<bool>),
    Byte(Vec<u8>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Long(Vec<i64>),
    Float(Vec<f32>),
    Double(Vec<f64>),
    NodeId(Vec<RawNodeId>),
    StringId(Vec<RawStringId>),
    Enum(Vec<RawEnumId>),
    #[default]
    None,
}

impl StorageArray {
    pub fn new(typ: PropertyType) -> Self {
        match typ {
            PropertyType::None => Self::None,
            PropertyType::Bool => Self::Bool(Vec::new()),
            PropertyType::Byte => Self::Byte(Vec::new()),
            PropertyType::Short => Self::Short(Vec::new()),
            PropertyType::Int => Self::Int(Vec::new()),
            PropertyType::Long => Self::Long(Vec::new()),
            PropertyType::Float => Self::Float(Vec::new()),
            PropertyType::Double => Self::Double(Vec::new()),
            PropertyType::NodeId => Self::NodeId(Vec::new()),
            PropertyType::String => Self::StringId(Vec::new()),
            PropertyType::Enum => Self::Enum(Vec::new()),
        }
    }

    pub fn with_capacity(typ: PropertyType, capacity: usize) -> Self {
        match typ {
            PropertyType::None => Self::None,
            PropertyType::Bool => Self::Bool(Vec::with_capacity(capacity)),
            PropertyType::Byte => Self::Byte(Vec::with_capacity(capacity)),
            PropertyType::Short => Self::Short(Vec::with_capacity(capacity)),
            PropertyType::Int => Self::Int(Vec::with_capacity(capacity)),
            PropertyType::Long => Self::Long(Vec::with_capacity(capacity)),
            PropertyType::Float => Self::Float(Vec::with_capacity(capacity)),
            PropertyType::Double => Self::Double(Vec::with_capacity(capacity)),
            PropertyType::NodeId => Self::NodeId(Vec::with_capacity(capacity)),
            PropertyType::String => Self::StringId(Vec::with_capacity(capacity)),
            PropertyType::Enum => Self::Enum(Vec::with_capacity(capacity)),
        }
    }

    pub fn typ(&self) -> PropertyType {
        match self {
            StorageArray::Bool(_) => PropertyType::Bool,
            StorageArray::Byte(_) => PropertyType::Byte,
            StorageArray::Short(_) => PropertyType::Short,
            StorageArray::Int(_) => PropertyType::Int,
            StorageArray::Long(_) => PropertyType::Long,
            StorageArray::Float(_) => PropertyType::Float,
            StorageArray::Double(_) => PropertyType::Double,
            StorageArray::NodeId(_) => PropertyType::NodeId,
            StorageArray::StringId(_) => PropertyType::String,
            StorageArray::Enum(_) => PropertyType::Enum,
            StorageArray::None => PropertyType::None,
        }
    }

    pub fn get(&self, index: usize) -> Option<StoredProperty> {
        match self {
            StorageArray::Bool(v) => v.get(index).copied().map(StoredProperty::Bool),
            StorageArray::Byte(v) => v.get(index).copied().map(StoredProperty::Byte),
            StorageArray::Short(v) => v.get(index).copied().map(StoredProperty::Short),
            StorageArray::Int(v) => v.get(index).copied().map(StoredProperty::Int),
            StorageArray::Long(v) => v.get(index).copied().map(StoredProperty::Long),
            StorageArray::Float(v) => v.get(index).copied().map(StoredProperty::Float),
            StorageArray::Double(v) => v.get(index).copied().map(StoredProperty::Double),
            StorageArray::NodeId(v) => v.get(index).cloned().map(StoredProperty::NodeId),
            StorageArray::StringId(v) => v.get(index).cloned().map(StoredProperty::StringId),
            StorageArray::Enum(v) => v.get(index).copied().map(StoredProperty::Enum),
            StorageArray::None => None,
        }
    }

    pub fn iter_range(&self, range: std::ops::Range<usize>) -> StorageArrayIter<'_> {
        match self {
            StorageArray::Bool(v) => StorageArrayIter::Bool(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::Byte(v) => StorageArrayIter::Byte(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::Short(v) => StorageArrayIter::Short(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::Int(v) => StorageArrayIter::Int(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::Long(v) => StorageArrayIter::Long(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::Float(v) => StorageArrayIter::Float(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::Double(v) => StorageArrayIter::Double(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::NodeId(v) => StorageArrayIter::NodeId(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::StringId(v) => {
                StorageArrayIter::StringId(v.get(range).unwrap_or(&[]).iter())
            }
            StorageArray::Enum(v) => StorageArrayIter::Enum(v.get(range).unwrap_or(&[]).iter()),
            StorageArray::None => StorageArrayIter::Empty,
        }
    }

    pub fn try_push(&mut self, value: &StoredProperty) -> Result<(), Error> {
        let target_typ = self.typ();
        let other_typ = value.typ();
        match (self, value) {
            (StorageArray::Bool(storage), StoredProperty::Bool(v)) => storage.push(*v),
            (StorageArray::Byte(storage), StoredProperty::Byte(v)) => storage.push(*v),
            (StorageArray::Short(storage), StoredProperty::Short(v)) => storage.push(*v),
            (StorageArray::Int(storage), StoredProperty::Int(v)) => storage.push(*v),
            (StorageArray::Long(storage), StoredProperty::Long(v)) => storage.push(*v),
            (StorageArray::Float(storage), StoredProperty::Float(v)) => storage.push(*v),
            (StorageArray::Double(storage), StoredProperty::Double(v)) => storage.push(*v),
            (StorageArray::NodeId(storage), StoredProperty::NodeId(v)) => storage.push(*v),
            (StorageArray::StringId(storage), StoredProperty::StringId(v)) => storage.push(*v),
            (StorageArray::Enum(storage), StoredProperty::Enum(v)) => storage.push(*v),
            _ => return Err(Error::invalid_property_type(target_typ, other_typ)),
        }
        Ok(())
    }

    pub fn try_append(&mut self, other: &mut StorageArray) -> Result<(), Error> {
        let target_typ = self.typ();
        let other_typ = other.typ();
        match (self, other) {
            (StorageArray::Bool(storage), StorageArray::Bool(v)) => storage.append(v),
            (StorageArray::Byte(storage), StorageArray::Byte(v)) => storage.append(v),
            (StorageArray::Short(storage), StorageArray::Short(v)) => storage.append(v),
            (StorageArray::Int(storage), StorageArray::Int(v)) => storage.append(v),
            (StorageArray::Long(storage), StorageArray::Long(v)) => storage.append(v),
            (StorageArray::Float(storage), StorageArray::Float(v)) => storage.append(v),
            (StorageArray::Double(storage), StorageArray::Double(v)) => storage.append(v),
            (StorageArray::NodeId(storage), StorageArray::NodeId(v)) => storage.append(v),
            (StorageArray::StringId(storage), StorageArray::StringId(v)) => storage.append(v),
            (StorageArray::Enum(storage), StorageArray::Enum(v)) => storage.append(v),
            (StorageArray::None, StorageArray::None) => (),
            _ => return Err(Error::invalid_property_type(target_typ, other_typ)),
        }
        Ok(())
    }

    pub(crate) fn try_extend_from_range(
        &mut self,
        src: &StorageArray,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let target_typ = self.typ();
        let src_typ = src.typ();
        match (self, src) {
            (Self::Bool(dst), Self::Bool(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::Byte(dst), Self::Byte(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::Short(dst), Self::Short(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::Int(dst), Self::Int(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::Long(dst), Self::Long(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::Float(dst), Self::Float(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::Double(dst), Self::Double(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::NodeId(dst), Self::NodeId(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::StringId(dst), Self::StringId(v)) => {
                dst.extend_from_slice(ranged_slice(v, range)?)
            }
            (Self::Enum(dst), Self::Enum(v)) => dst.extend_from_slice(ranged_slice(v, range)?),
            (Self::None, Self::None) => (),
            _ => return Err(Error::invalid_property_type(target_typ, src_typ)),
        }
        Ok(())
    }

    pub fn try_splice(&mut self, at: usize, other: StorageArray) -> Result<(), Error> {
        let target_typ = self.typ();
        let other_typ = other.typ();
        match (self, other) {
            (StorageArray::Bool(storage), StorageArray::Bool(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Byte(storage), StorageArray::Byte(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Short(storage), StorageArray::Short(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Int(storage), StorageArray::Int(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Long(storage), StorageArray::Long(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Float(storage), StorageArray::Float(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Double(storage), StorageArray::Double(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::NodeId(storage), StorageArray::NodeId(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::StringId(storage), StorageArray::StringId(v)) => {
                storage.splice(at..at, v);
            }
            (StorageArray::Enum(storage), StorageArray::Enum(v)) => {
                storage.splice(at..at, v);
            }
            _ => return Err(Error::invalid_property_type(target_typ, other_typ)),
        }
        Ok(())
    }

    pub fn try_insert(&mut self, i: usize, other: &StoredProperty) -> Result<(), Error> {
        let target_typ = self.typ();
        let other_typ = other.typ();
        match (self, other) {
            (StorageArray::Bool(storage), StoredProperty::Bool(v)) => storage.insert(i, *v),
            (StorageArray::Byte(storage), StoredProperty::Byte(v)) => storage.insert(i, *v),
            (StorageArray::Short(storage), StoredProperty::Short(v)) => storage.insert(i, *v),
            (StorageArray::Int(storage), StoredProperty::Int(v)) => storage.insert(i, *v),
            (StorageArray::Long(storage), StoredProperty::Long(v)) => storage.insert(i, *v),
            (StorageArray::Float(storage), StoredProperty::Float(v)) => storage.insert(i, *v),
            (StorageArray::Double(storage), StoredProperty::Double(v)) => storage.insert(i, *v),
            (StorageArray::NodeId(storage), StoredProperty::NodeId(v)) => storage.insert(i, *v),
            (StorageArray::StringId(storage), StoredProperty::StringId(v)) => storage.insert(i, *v),
            (StorageArray::Enum(storage), StoredProperty::Enum(v)) => storage.insert(i, *v),
            _ => return Err(Error::invalid_property_type(target_typ, other_typ)),
        }
        Ok(())
    }

    pub fn try_drain(&mut self, range: std::ops::Range<usize>) -> Result<(), Error> {
        match self {
            StorageArray::Bool(v) => {
                v.drain(range);
            }
            StorageArray::Byte(v) => {
                v.drain(range);
            }
            StorageArray::Short(v) => {
                v.drain(range);
            }
            StorageArray::Int(v) => {
                v.drain(range);
            }
            StorageArray::Long(v) => {
                v.drain(range);
            }
            StorageArray::Float(v) => {
                v.drain(range);
            }
            StorageArray::Double(v) => {
                v.drain(range);
            }
            StorageArray::NodeId(v) => {
                v.drain(range);
            }
            StorageArray::StringId(v) => {
                v.drain(range);
            }
            StorageArray::Enum(v) => {
                v.drain(range);
            }
            StorageArray::None => {}
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        match self {
            StorageArray::Bool(items) => items.len(),
            StorageArray::Byte(items) => items.len(),
            StorageArray::Short(items) => items.len(),
            StorageArray::Int(items) => items.len(),
            StorageArray::Long(items) => items.len(),
            StorageArray::Float(items) => items.len(),
            StorageArray::Double(items) => items.len(),
            StorageArray::NodeId(node_ref) => node_ref.len(),
            StorageArray::StringId(items) => items.len(),
            StorageArray::Enum(items) => items.len(),
            StorageArray::None => 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn try_as_bool(&self) -> Result<&Vec<bool>, Error> {
        match self {
            Self::Bool(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Bool)),
        }
    }

    pub fn try_as_bool_mut(&mut self) -> Result<&mut Vec<bool>, Error> {
        match self {
            Self::Bool(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Bool)),
        }
    }

    pub fn try_as_byte(&self) -> Result<&Vec<u8>, Error> {
        match self {
            Self::Byte(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Byte)),
        }
    }

    pub fn try_as_byte_mut(&mut self) -> Result<&mut Vec<u8>, Error> {
        match self {
            Self::Byte(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Byte)),
        }
    }

    pub fn try_as_short(&self) -> Result<&Vec<i16>, Error> {
        match self {
            Self::Short(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Short)),
        }
    }

    pub fn try_as_short_mut(&mut self) -> Result<&mut Vec<i16>, Error> {
        match self {
            Self::Short(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Short)),
        }
    }

    pub fn try_as_int(&self) -> Result<&Vec<i32>, Error> {
        match self {
            Self::Int(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Int)),
        }
    }

    pub fn try_as_int_mut(&mut self) -> Result<&mut Vec<i32>, Error> {
        match self {
            Self::Int(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Int)),
        }
    }

    pub fn try_as_long(&self) -> Result<&Vec<i64>, Error> {
        match self {
            Self::Long(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Long)),
        }
    }

    pub fn try_as_long_mut(&mut self) -> Result<&mut Vec<i64>, Error> {
        match self {
            Self::Long(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Long)),
        }
    }

    pub fn try_as_float(&self) -> Result<&Vec<f32>, Error> {
        match self {
            Self::Float(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Float)),
        }
    }

    pub fn try_as_float_mut(&mut self) -> Result<&mut Vec<f32>, Error> {
        match self {
            Self::Float(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Float)),
        }
    }

    pub fn try_as_double(&self) -> Result<&Vec<f64>, Error> {
        match self {
            Self::Double(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Double)),
        }
    }

    pub fn try_as_double_mut(&mut self) -> Result<&mut Vec<f64>, Error> {
        match self {
            Self::Double(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Double)),
        }
    }

    pub fn try_as_node_id(&self) -> Result<&Vec<RawNodeId>, Error> {
        match self {
            Self::NodeId(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::NodeId)),
        }
    }

    pub fn try_as_node_id_mut(&mut self) -> Result<&mut Vec<RawNodeId>, Error> {
        match self {
            Self::NodeId(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::NodeId)),
        }
    }

    pub fn try_as_enum(&self) -> Result<&Vec<RawEnumId>, Error> {
        match self {
            Self::Enum(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Enum)),
        }
    }

    pub fn try_as_enum_mut(&mut self) -> Result<&mut Vec<RawEnumId>, Error> {
        match self {
            Self::Enum(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Enum)),
        }
    }

    pub fn try_as_string(&self) -> Result<&Vec<RawStringId>, Error> {
        match self {
            Self::StringId(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::String)),
        }
    }

    pub fn try_as_string_mut(&mut self) -> Result<&mut Vec<RawStringId>, Error> {
        match self {
            Self::StringId(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::String)),
        }
    }

    pub fn try_into_bool(self) -> Result<Vec<bool>, Error> {
        match self {
            Self::Bool(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Bool)),
        }
    }

    pub fn try_into_byte(self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Byte(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Byte)),
        }
    }

    pub fn try_into_short(self) -> Result<Vec<i16>, Error> {
        match self {
            Self::Short(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Short)),
        }
    }

    pub fn try_into_int(self) -> Result<Vec<i32>, Error> {
        match self {
            Self::Int(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Int)),
        }
    }

    pub fn try_into_long(self) -> Result<Vec<i64>, Error> {
        match self {
            Self::Long(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Long)),
        }
    }

    pub fn try_into_float(self) -> Result<Vec<f32>, Error> {
        match self {
            Self::Float(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Float)),
        }
    }

    pub fn try_into_double(self) -> Result<Vec<f64>, Error> {
        match self {
            Self::Double(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Double)),
        }
    }

    pub fn try_into_node_id(self) -> Result<Vec<RawNodeId>, Error> {
        match self {
            Self::NodeId(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::NodeId)),
        }
    }

    pub fn try_into_string(self) -> Result<Vec<RawStringId>, Error> {
        match self {
            Self::StringId(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::String)),
        }
    }

    pub fn try_into_enum(self) -> Result<Vec<RawEnumId>, Error> {
        match self {
            Self::Enum(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::Enum)),
        }
    }

    fn casting_error(&self, target: PropertyType) -> Error {
        Error::invalid_property_type(target, self.typ())
    }
}

pub enum StorageArrayIter<'a> {
    Bool(std::slice::Iter<'a, bool>),
    Byte(std::slice::Iter<'a, u8>),
    Short(std::slice::Iter<'a, i16>),
    Int(std::slice::Iter<'a, i32>),
    Long(std::slice::Iter<'a, i64>),
    Float(std::slice::Iter<'a, f32>),
    Double(std::slice::Iter<'a, f64>),
    NodeId(std::slice::Iter<'a, RawNodeId>),
    StringId(std::slice::Iter<'a, RawStringId>),
    Enum(std::slice::Iter<'a, RawEnumId>),
    Empty,
}

impl Iterator for StorageArrayIter<'_> {
    type Item = StoredProperty;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Bool(it) => it.next().copied().map(StoredProperty::Bool),
            Self::Byte(it) => it.next().copied().map(StoredProperty::Byte),
            Self::Short(it) => it.next().copied().map(StoredProperty::Short),
            Self::Int(it) => it.next().copied().map(StoredProperty::Int),
            Self::Long(it) => it.next().copied().map(StoredProperty::Long),
            Self::Float(it) => it.next().copied().map(StoredProperty::Float),
            Self::Double(it) => it.next().copied().map(StoredProperty::Double),
            Self::NodeId(it) => it.next().copied().map(StoredProperty::NodeId),
            Self::StringId(it) => it.next().copied().map(StoredProperty::StringId),
            Self::Enum(it) => it.next().copied().map(StoredProperty::Enum),
            Self::Empty => None,
        }
    }
}

pub struct NodeMetaStorage<S> {
    storage: Vec<Vec<NodeMeta>>,
    _phantom: PhantomData<S>,
}

impl<S: Schema> NodeMetaStorage<S> {
    pub fn new() -> Self {
        Self {
            storage: vec![Vec::default(); S::number_of_node_kinds()],
            _phantom: PhantomData,
        }
    }
}

impl<S: Schema> Default for NodeMetaStorage<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Deref for NodeMetaStorage<S> {
    type Target = Vec<Vec<NodeMeta>>;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl<S> DerefMut for NodeMetaStorage<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage
    }
}

pub trait OffsetStorage {
    fn offsets(&self) -> &Vec<Offset>;
    fn offsets_mut(&mut self) -> &mut Vec<Offset>;
    fn get_offset(&self, index: usize) -> Option<(Offset, Offset)> {
        match self.offsets().get(index..=(index + 1)) {
            Some([start, end]) => Some((*start, *end)),
            _ => None,
        }
    }
}

#[derive(Default, Clone)]
pub struct EdgeStorageSlot {
    offsets: Vec<Offset>,
    neighbors: Vec<RawNodeId>,
    values: StorageArray,
}

impl EdgeStorageSlot {
    pub fn get_neighbors(&self, start: Offset, end: Offset) -> impl Iterator<Item = RawNodeId> {
        self.neighbors
            .get(start.value()..end.value())
            .unwrap_or(&[])
            .iter()
            .copied()
    }

    pub fn neighbors(&self) -> &Vec<RawNodeId> {
        &self.neighbors
    }

    pub fn neighbors_mut(&mut self) -> &mut Vec<RawNodeId> {
        &mut self.neighbors
    }

    pub fn get_value(&self, index: Offset) -> Option<StoredProperty> {
        self.values.get(index.value())
    }

    pub fn values(&self) -> &StorageArray {
        &self.values
    }

    pub fn values_mut(&mut self) -> &mut StorageArray {
        &mut self.values
    }
}

impl OffsetStorage for EdgeStorageSlot {
    fn offsets(&self) -> &Vec<Offset> {
        &self.offsets
    }

    fn offsets_mut(&mut self) -> &mut Vec<Offset> {
        &mut self.offsets
    }
}
pub struct EdgeStorage<S> {
    storage: Vec<EdgeStorageSlot>,
    _phantom: PhantomData<S>,
}

impl<S: Schema> EdgeStorage<S> {
    pub fn new() -> Self {
        let mut storage = vec![EdgeStorageSlot::default(); S::edge_storage_size()];

        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let slot_index = S::edge_storage_slot(node_kind, direction, edge_kind);
            let slot = &mut storage[slot_index.index()];

            slot.values = StorageArray::new(S::edge_property_type(edge_kind));
        }
        Self {
            storage,
            _phantom: PhantomData,
        }
    }
}

impl<S: Schema> Default for EdgeStorage<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Deref for EdgeStorage<S> {
    type Target = Vec<EdgeStorageSlot>;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl<S> DerefMut for EdgeStorage<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage
    }
}

impl<'a, S> IntoIterator for &'a EdgeStorage<S> {
    type Item = &'a EdgeStorageSlot;

    type IntoIter = std::slice::Iter<'a, EdgeStorageSlot>;

    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}

#[derive(Default, Clone)]
pub struct PropertyStorageSlot {
    offsets: Vec<Offset>,
    values: StorageArray,
}

impl PropertyStorageSlot {
    pub fn get_values(&self, start: Offset, end: Offset) -> StorageArrayIter<'_> {
        self.values.iter_range(start.value()..end.value())
    }

    pub fn values(&self) -> &StorageArray {
        &self.values
    }

    pub fn values_mut(&mut self) -> &mut StorageArray {
        &mut self.values
    }
}

impl OffsetStorage for PropertyStorageSlot {
    fn offsets(&self) -> &Vec<Offset> {
        &self.offsets
    }

    fn offsets_mut(&mut self) -> &mut Vec<Offset> {
        &mut self.offsets
    }
}

pub struct PropertyStorage<S> {
    storage: Vec<PropertyStorageSlot>,
    _phantom: PhantomData<S>,
}

impl<S: Schema> PropertyStorage<S> {
    pub fn new() -> Self {
        let mut storage = vec![PropertyStorageSlot::default(); S::property_storage_size()];

        for (node_kind, property_kind) in S::property_storage_slots_iter() {
            let slot_index = S::property_storage_slot(node_kind, property_kind);
            let slot = &mut storage[slot_index.index()];

            slot.values = StorageArray::new(S::node_property_type(property_kind));
        }
        Self {
            storage,
            _phantom: PhantomData,
        }
    }
}

impl<S: Schema> Default for PropertyStorage<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Deref for PropertyStorage<S> {
    type Target = Vec<PropertyStorageSlot>;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl<S> DerefMut for PropertyStorage<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage
    }
}

impl<'a, S> IntoIterator for &'a PropertyStorage<S> {
    type Item = &'a PropertyStorageSlot;

    type IntoIter = std::slice::Iter<'a, PropertyStorageSlot>;

    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::mem::discriminant;

    use super::*;
    use crate::strings_pool::StringsPool;

    fn samples() -> Vec<StoredProperty> {
        let mut strings = StringsPool::new();
        vec![
            StoredProperty::Bool(true),
            StoredProperty::Byte(7),
            StoredProperty::Short(7),
            StoredProperty::Int(7),
            StoredProperty::Long(7),
            StoredProperty::Float(7.0),
            StoredProperty::Double(7.0),
            StoredProperty::NodeId(RawNodeId::new(1, 2)),
            StoredProperty::StringId(strings.intern("x")),
            StoredProperty::Enum(RawEnumId::new(0, 0)),
        ]
    }

    #[test]
    fn stored_property_typ_matches_each_variant() {
        let expected = [
            PropertyType::Bool,
            PropertyType::Byte,
            PropertyType::Short,
            PropertyType::Int,
            PropertyType::Long,
            PropertyType::Float,
            PropertyType::Double,
            PropertyType::NodeId,
            PropertyType::String,
            PropertyType::Enum,
        ];
        for (sample, typ) in samples().iter().zip(expected) {
            assert_eq!(sample.typ(), typ);
        }
    }

    #[test]
    fn offset_new_rejects_values_beyond_inner_offset_range() {
        assert_eq!(Offset::new(5).unwrap().value(), 5);
        let err = Offset::new(u32::MAX as usize + 1).unwrap_err();
        assert!(matches!(err, Error::OffsetOverflow(_)));
    }

    #[test]
    fn offset_zero_has_zero_value() {
        assert_eq!(Offset::zero().value(), 0);
    }

    #[test]
    fn offset_checked_sub_computes_length_and_rejects_decrease() {
        let a = Offset::new(5).unwrap();
        let b = Offset::new(2).unwrap();
        assert_eq!(a.checked_sub(b).unwrap(), 3);
        assert!(matches!(
            b.checked_sub(a).unwrap_err(),
            Error::OffsetUnderflow
        ));
    }

    #[test]
    fn offset_checked_add_delta_adds_and_rejects_overflow() {
        let a = Offset::zero();
        assert_eq!(a.checked_add_delta(5).unwrap().value(), 5);
        let max = Offset::new(u32::MAX as usize).unwrap();
        assert!(matches!(
            max.checked_add_delta(1).unwrap_err(),
            Error::OffsetOverflow(_)
        ));
        assert!(matches!(
            a.checked_add_delta(u32::MAX as usize + 1).unwrap_err(),
            Error::OffsetOverflow(_)
        ));
    }

    #[test]
    fn offset_checked_sub_delta_subtracts_and_rejects_underflow() {
        let a = Offset::new(5).unwrap();
        assert_eq!(a.checked_sub_delta(2).unwrap().value(), 3);
        assert!(matches!(
            Offset::zero().checked_sub_delta(1).unwrap_err(),
            Error::OffsetUnderflow
        ));
    }

    #[test]
    fn usize_from_offset_ref_matches_value() {
        let offset = Offset::new(9).unwrap();
        assert_eq!(usize::from(&offset), 9);
    }

    #[test]
    fn storage_array_new_and_with_capacity_match_the_requested_type() {
        for typ in [
            PropertyType::None,
            PropertyType::Bool,
            PropertyType::Byte,
            PropertyType::Short,
            PropertyType::Int,
            PropertyType::Long,
            PropertyType::Float,
            PropertyType::Double,
            PropertyType::NodeId,
            PropertyType::String,
            PropertyType::Enum,
        ] {
            assert_eq!(StorageArray::new(typ).typ(), typ);
            assert_eq!(StorageArray::with_capacity(typ, 4).typ(), typ);
        }
    }

    #[test]
    fn storage_array_none_reports_len_one_and_is_not_empty() {
        let arr = StorageArray::new(PropertyType::None);
        assert_eq!(arr.len(), 1);
        assert!(!arr.is_empty());
    }

    #[test]
    fn storage_array_try_push_get_and_len_round_trip_each_type() {
        for sample in samples() {
            let mut arr = StorageArray::new(sample.typ());
            assert!(arr.is_empty());
            arr.try_push(&sample).unwrap();
            assert_eq!(arr.len(), 1);
            assert!(!arr.is_empty());
            let got = arr.get(0).unwrap();
            assert_eq!(discriminant(&got), discriminant(&sample));
            assert!(arr.get(1).is_none());
        }
    }

    #[test]
    fn storage_array_try_push_rejects_mismatched_type() {
        let mut arr = StorageArray::new(PropertyType::Bool);
        let err = arr.try_push(&StoredProperty::Int(1)).unwrap_err();
        assert!(matches!(err, Error::InvalidPropertyType { .. }));
    }

    #[test]
    fn storage_array_try_append_moves_elements_out_of_the_source() {
        for sample in samples() {
            let mut dst = StorageArray::new(sample.typ());
            let mut src = StorageArray::new(sample.typ());
            src.try_push(&sample).unwrap();
            src.try_push(&sample).unwrap();
            dst.try_append(&mut src).unwrap();
            assert_eq!(dst.len(), 2);
            assert_eq!(src.len(), 0);
        }
        let mut none_dst = StorageArray::new(PropertyType::None);
        let mut none_src = StorageArray::new(PropertyType::None);
        none_dst.try_append(&mut none_src).unwrap();
    }

    #[test]
    fn storage_array_try_append_rejects_mismatched_type() {
        let mut dst = StorageArray::new(PropertyType::Bool);
        let mut src = StorageArray::new(PropertyType::Int);
        let err = dst.try_append(&mut src).unwrap_err();
        assert!(matches!(err, Error::InvalidPropertyType { .. }));
    }

    #[test]
    fn storage_array_try_extend_from_range_copies_a_slice() {
        for sample in samples() {
            let mut src = StorageArray::new(sample.typ());
            for _ in 0..3 {
                src.try_push(&sample).unwrap();
            }
            let mut dst = StorageArray::new(sample.typ());
            dst.try_extend_from_range(&src, 0..2).unwrap();
            assert_eq!(dst.len(), 2);
        }
    }

    #[test]
    fn storage_array_try_extend_from_range_rejects_mismatched_type_and_bad_range() {
        let src = StorageArray::new(PropertyType::Int);
        let mut bool_dst = StorageArray::new(PropertyType::Bool);
        assert!(matches!(
            bool_dst.try_extend_from_range(&src, 0..0).unwrap_err(),
            Error::InvalidPropertyType { .. }
        ));

        let mut int_dst = StorageArray::new(PropertyType::Int);
        assert!(matches!(
            int_dst.try_extend_from_range(&src, 0..5).unwrap_err(),
            Error::PropertyIndexOutOfBounds { .. }
        ));

        let mut none_dst = StorageArray::new(PropertyType::None);
        let none_src = StorageArray::new(PropertyType::None);
        none_dst.try_extend_from_range(&none_src, 0..0).unwrap();
    }

    #[test]
    fn storage_array_try_splice_inserts_at_the_given_position() {
        for sample in samples() {
            let mut arr = StorageArray::new(sample.typ());
            arr.try_push(&sample).unwrap();
            arr.try_push(&sample).unwrap();
            let mut middle = StorageArray::new(sample.typ());
            middle.try_push(&sample).unwrap();
            arr.try_splice(1, middle).unwrap();
            assert_eq!(arr.len(), 3);
        }
    }

    #[test]
    fn storage_array_try_splice_rejects_mismatched_type() {
        let mut dst = StorageArray::new(PropertyType::Bool);
        let src = StorageArray::new(PropertyType::Int);
        let err = dst.try_splice(0, src).unwrap_err();
        assert!(matches!(err, Error::InvalidPropertyType { .. }));
    }

    #[test]
    fn storage_array_try_insert_places_the_value_at_the_given_index() {
        for sample in samples() {
            let mut arr = StorageArray::new(sample.typ());
            arr.try_push(&sample).unwrap();
            arr.try_insert(0, &sample).unwrap();
            assert_eq!(arr.len(), 2);
        }
    }

    #[test]
    fn storage_array_try_insert_rejects_mismatched_type() {
        let mut arr = StorageArray::new(PropertyType::Bool);
        let err = arr.try_insert(0, &StoredProperty::Int(1)).unwrap_err();
        assert!(matches!(err, Error::InvalidPropertyType { .. }));
    }

    #[test]
    fn storage_array_try_drain_removes_the_given_range() {
        for sample in samples() {
            let mut arr = StorageArray::new(sample.typ());
            for _ in 0..3 {
                arr.try_push(&sample).unwrap();
            }
            arr.try_drain(0..2).unwrap();
            assert_eq!(arr.len(), 1);
        }
        let mut none_arr = StorageArray::new(PropertyType::None);
        none_arr.try_drain(0..0).unwrap();
    }

    #[test]
    fn storage_array_iter_range_yields_the_requested_slice_and_empty_past_the_end() {
        for sample in samples() {
            let mut arr = StorageArray::new(sample.typ());
            for _ in 0..3 {
                arr.try_push(&sample).unwrap();
            }
            assert_eq!(arr.iter_range(0..2).count(), 2);
            assert_eq!(arr.iter_range(10..20).count(), 0);
        }
    }

    #[test]
    fn storage_array_iter_range_over_none_is_always_empty() {
        let arr = StorageArray::new(PropertyType::None);
        let mut iter = arr.iter_range(0..5);
        assert!(iter.next().is_none());
    }

    #[test]
    fn storage_array_try_as_succeeds_for_matching_type_and_fails_otherwise() {
        let mismatch = StorageArray::new(PropertyType::None);

        let mut bool_arr = StorageArray::new(PropertyType::Bool);
        bool_arr.try_push(&StoredProperty::Bool(true)).unwrap();
        assert_eq!(bool_arr.try_as_bool().unwrap(), &vec![true]);
        assert_eq!(bool_arr.try_as_bool_mut().unwrap(), &mut vec![true]);
        assert!(mismatch.try_as_bool().is_err());
        assert!(
            StorageArray::new(PropertyType::Bool)
                .try_as_bool_mut()
                .is_ok()
        );
        assert!(mismatch.clone().try_as_bool_mut().is_err());

        let mut byte_arr = StorageArray::new(PropertyType::Byte);
        byte_arr.try_push(&StoredProperty::Byte(7)).unwrap();
        assert_eq!(byte_arr.try_as_byte().unwrap(), &vec![7]);
        assert_eq!(byte_arr.try_as_byte_mut().unwrap(), &mut vec![7]);
        assert!(mismatch.try_as_byte().is_err());
        assert!(mismatch.clone().try_as_byte_mut().is_err());

        let mut short_arr = StorageArray::new(PropertyType::Short);
        short_arr.try_push(&StoredProperty::Short(7)).unwrap();
        assert_eq!(short_arr.try_as_short().unwrap(), &vec![7]);
        assert_eq!(short_arr.try_as_short_mut().unwrap(), &mut vec![7]);
        assert!(mismatch.try_as_short().is_err());
        assert!(mismatch.clone().try_as_short_mut().is_err());

        let mut int_arr = StorageArray::new(PropertyType::Int);
        int_arr.try_push(&StoredProperty::Int(7)).unwrap();
        assert_eq!(int_arr.try_as_int().unwrap(), &vec![7]);
        assert_eq!(int_arr.try_as_int_mut().unwrap(), &mut vec![7]);
        assert!(mismatch.try_as_int().is_err());
        assert!(mismatch.clone().try_as_int_mut().is_err());

        let mut long_arr = StorageArray::new(PropertyType::Long);
        long_arr.try_push(&StoredProperty::Long(7)).unwrap();
        assert_eq!(long_arr.try_as_long().unwrap(), &vec![7]);
        assert_eq!(long_arr.try_as_long_mut().unwrap(), &mut vec![7]);
        assert!(mismatch.try_as_long().is_err());
        assert!(mismatch.clone().try_as_long_mut().is_err());

        let mut float_arr = StorageArray::new(PropertyType::Float);
        float_arr.try_push(&StoredProperty::Float(7.0)).unwrap();
        assert_eq!(float_arr.try_as_float().unwrap(), &vec![7.0]);
        assert_eq!(float_arr.try_as_float_mut().unwrap(), &mut vec![7.0]);
        assert!(mismatch.try_as_float().is_err());
        assert!(mismatch.clone().try_as_float_mut().is_err());

        let mut double_arr = StorageArray::new(PropertyType::Double);
        double_arr.try_push(&StoredProperty::Double(7.0)).unwrap();
        assert_eq!(double_arr.try_as_double().unwrap(), &vec![7.0]);
        assert_eq!(double_arr.try_as_double_mut().unwrap(), &mut vec![7.0]);
        assert!(mismatch.try_as_double().is_err());
        assert!(mismatch.clone().try_as_double_mut().is_err());

        let raw_node = RawNodeId::new(1, 2);
        let mut node_arr = StorageArray::new(PropertyType::NodeId);
        node_arr
            .try_push(&StoredProperty::NodeId(raw_node))
            .unwrap();
        assert_eq!(node_arr.try_as_node_id().unwrap(), &vec![raw_node]);
        assert_eq!(node_arr.try_as_node_id_mut().unwrap(), &mut vec![raw_node]);
        assert!(mismatch.try_as_node_id().is_err());
        assert!(mismatch.clone().try_as_node_id_mut().is_err());

        let raw_enum = RawEnumId::new(0, 0);
        let mut enum_arr = StorageArray::new(PropertyType::Enum);
        enum_arr.try_push(&StoredProperty::Enum(raw_enum)).unwrap();
        assert_eq!(enum_arr.try_as_enum().unwrap(), &vec![raw_enum]);
        assert_eq!(enum_arr.try_as_enum_mut().unwrap(), &mut vec![raw_enum]);
        assert!(mismatch.try_as_enum().is_err());
        assert!(mismatch.clone().try_as_enum_mut().is_err());

        let mut strings = StringsPool::new();
        let raw_string = strings.intern("x");
        let mut string_arr = StorageArray::new(PropertyType::String);
        string_arr
            .try_push(&StoredProperty::StringId(raw_string))
            .unwrap();
        assert_eq!(string_arr.try_as_string().unwrap(), &vec![raw_string]);
        assert_eq!(
            string_arr.try_as_string_mut().unwrap(),
            &mut vec![raw_string]
        );
        assert!(mismatch.try_as_string().is_err());
        assert!(mismatch.clone().try_as_string_mut().is_err());
    }

    #[test]
    fn storage_array_try_into_converts_ownership_and_fails_otherwise() {
        let mut bool_arr = StorageArray::new(PropertyType::Bool);
        bool_arr.try_push(&StoredProperty::Bool(true)).unwrap();
        assert_eq!(bool_arr.try_into_bool().unwrap(), vec![true]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_bool()
                .is_err()
        );

        let mut byte_arr = StorageArray::new(PropertyType::Byte);
        byte_arr.try_push(&StoredProperty::Byte(7)).unwrap();
        assert_eq!(byte_arr.try_into_byte().unwrap(), vec![7]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_byte()
                .is_err()
        );

        let mut short_arr = StorageArray::new(PropertyType::Short);
        short_arr.try_push(&StoredProperty::Short(7)).unwrap();
        assert_eq!(short_arr.try_into_short().unwrap(), vec![7]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_short()
                .is_err()
        );

        let mut int_arr = StorageArray::new(PropertyType::Int);
        int_arr.try_push(&StoredProperty::Int(7)).unwrap();
        assert_eq!(int_arr.try_into_int().unwrap(), vec![7]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_int()
                .is_err()
        );

        let mut long_arr = StorageArray::new(PropertyType::Long);
        long_arr.try_push(&StoredProperty::Long(7)).unwrap();
        assert_eq!(long_arr.try_into_long().unwrap(), vec![7]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_long()
                .is_err()
        );

        let mut float_arr = StorageArray::new(PropertyType::Float);
        float_arr.try_push(&StoredProperty::Float(7.0)).unwrap();
        assert_eq!(float_arr.try_into_float().unwrap(), vec![7.0]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_float()
                .is_err()
        );

        let mut double_arr = StorageArray::new(PropertyType::Double);
        double_arr.try_push(&StoredProperty::Double(7.0)).unwrap();
        assert_eq!(double_arr.try_into_double().unwrap(), vec![7.0]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_double()
                .is_err()
        );

        let raw_node = RawNodeId::new(1, 2);
        let mut node_arr = StorageArray::new(PropertyType::NodeId);
        node_arr
            .try_push(&StoredProperty::NodeId(raw_node))
            .unwrap();
        assert_eq!(node_arr.try_into_node_id().unwrap(), vec![raw_node]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_node_id()
                .is_err()
        );

        let raw_enum = RawEnumId::new(0, 0);
        let mut enum_arr = StorageArray::new(PropertyType::Enum);
        enum_arr.try_push(&StoredProperty::Enum(raw_enum)).unwrap();
        assert_eq!(enum_arr.try_into_enum().unwrap(), vec![raw_enum]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_enum()
                .is_err()
        );

        let mut strings = StringsPool::new();
        let raw_string = strings.intern("x");
        let mut string_arr = StorageArray::new(PropertyType::String);
        string_arr
            .try_push(&StoredProperty::StringId(raw_string))
            .unwrap();
        assert_eq!(string_arr.try_into_string().unwrap(), vec![raw_string]);
        assert!(
            StorageArray::new(PropertyType::None)
                .try_into_string()
                .is_err()
        );
    }
}
