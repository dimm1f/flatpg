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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset(InnerOffset);

impl Offset {
    pub fn new(value: usize) -> Result<Self, Error> {
        InnerOffset::try_from(value)
            .map(Self)
            .map_err(|_| Error::offset_overflow(value))
    }

    pub fn zero() -> Self {
        Self(0)
    }

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

#[derive(Debug, Clone, Default)]
pub enum StorageArray {
    Bool(Vec<bool>),
    Byte(Vec<u8>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Offset(Vec<Offset>),
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

    // TODO: Offsets should be stored separately with properties.
    // This can be implemented by defining slots in EgdeStorage
    // and PropertyStorage as Struct of Arrays.
    pub fn new_offsets() -> Self {
        Self::Offset(Vec::new())
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
            StorageArray::Offset(_) => PropertyType::None,
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
            StorageArray::Offset(_) => None,
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
            StorageArray::Offset(_) => StorageArrayIter::Empty,
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
            StorageArray::Offset(v) => {
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
            StorageArray::Offset(items) => items.len(),
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

    pub fn try_as_offset(&self) -> Result<&Vec<Offset>, Error> {
        match self {
            Self::Offset(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::None)),
        }
    }

    pub fn try_as_offset_mut(&mut self) -> Result<&mut Vec<Offset>, Error> {
        match self {
            Self::Offset(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::None)),
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

    pub fn try_into_offset(self) -> Result<Vec<Offset>, Error> {
        match self {
            Self::Offset(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::None)),
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

pub struct EdgeStorage<S> {
    storage: Vec<StorageArray>,
    _phantom: PhantomData<S>,
}

impl<S: Schema> EdgeStorage<S> {
    pub fn new() -> Self {
        let mut storage = vec![StorageArray::default(); S::edge_storage_size()];

        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

            // Safety: storage has edge_storage_size() slots; slot guarantees all three indices are in-bounds and pairwise distinct.
            let [offsets, neighbors, properties] = unsafe {
                storage.get_disjoint_unchecked_mut([
                    slot.offset_index(),
                    slot.neighbors_index(),
                    slot.properties_index(),
                ])
            };
            *offsets = StorageArray::new_offsets();
            *neighbors = StorageArray::new(PropertyType::NodeId);
            *properties = StorageArray::new(S::edge_property_type(edge_kind));
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
    type Target = Vec<StorageArray>;

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
    type Item = &'a StorageArray;

    type IntoIter = std::slice::Iter<'a, StorageArray>;

    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}

pub struct PropertyStorage<S> {
    storage: Vec<StorageArray>,
    _phantom: PhantomData<S>,
}

impl<S: Schema> PropertyStorage<S> {
    pub fn new() -> Self {
        let mut storage = vec![StorageArray::default(); S::property_storage_size()];

        for (node_kind, property_kind) in S::property_storage_slots_iter() {
            let slot = S::property_storage_slot(node_kind, property_kind);

            // Safety: storage has property_storage_size() slots; slot guarantees both indices are in-bounds and distinct.
            let [offsets, values] = unsafe {
                storage.get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
            };
            *offsets = StorageArray::new_offsets();
            *values = StorageArray::new(S::node_property_type(property_kind));
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
    type Target = Vec<StorageArray>;

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
    type Item = &'a StorageArray;

    type IntoIter = std::slice::Iter<'a, StorageArray>;

    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}
