use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{
    ItemIndex,
    error::Error,
    node::{NodeMeta, NodeRef},
    property::PropertyType,
    schema::Schema,
    strings_pool::StringRef,
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
    NodeRef(NodeRef),
    StringRef(StringRef),
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
            StoredProperty::NodeRef(_) => PropertyType::NodeRef,
            StoredProperty::StringRef(_) => PropertyType::String,
        }
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
    NodeRef(Vec<NodeRef>),
    StringRef(Vec<StringRef>),
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
            PropertyType::NodeRef => Self::NodeRef(Vec::new()),
            PropertyType::String => Self::StringRef(Vec::new()),
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
            StorageArray::NodeRef(_) => PropertyType::NodeRef,
            StorageArray::StringRef(_) => PropertyType::String,
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
            StorageArray::NodeRef(v) => v.get(index).cloned().map(StoredProperty::NodeRef),
            StorageArray::StringRef(v) => v.get(index).cloned().map(StoredProperty::StringRef),
            StorageArray::None => None,
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
            (StorageArray::NodeRef(storage), StoredProperty::NodeRef(v)) => storage.push(*v),
            (StorageArray::StringRef(storage), StoredProperty::StringRef(v)) => storage.push(*v),
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
            (StorageArray::NodeRef(storage), StorageArray::NodeRef(v)) => storage.append(v),
            (StorageArray::StringRef(storage), StorageArray::StringRef(v)) => storage.append(v),
            (StorageArray::None, StorageArray::None) => (),
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
            (StorageArray::NodeRef(storage), StoredProperty::NodeRef(v)) => storage.insert(i, *v),
            (StorageArray::StringRef(storage), StoredProperty::StringRef(v)) => {
                storage.insert(i, *v)
            }
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
            StorageArray::NodeRef(v) => {
                v.drain(range);
            }
            StorageArray::StringRef(v) => {
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
            StorageArray::NodeRef(node_ref) => node_ref.len(),
            StorageArray::StringRef(items) => items.len(),
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

    pub fn try_as_ref(&self) -> Result<&Vec<NodeRef>, Error> {
        match self {
            Self::NodeRef(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::NodeRef)),
        }
    }

    pub fn try_as_ref_mut(&mut self) -> Result<&mut Vec<NodeRef>, Error> {
        match self {
            Self::NodeRef(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::NodeRef)),
        }
    }

    pub fn try_as_string(&self) -> Result<&Vec<StringRef>, Error> {
        match self {
            Self::StringRef(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::String)),
        }
    }

    pub fn try_as_string_mut(&mut self) -> Result<&mut Vec<StringRef>, Error> {
        match self {
            Self::StringRef(items) => Ok(items),
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

    pub fn try_into_ref(self) -> Result<Vec<NodeRef>, Error> {
        match self {
            Self::NodeRef(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::NodeRef)),
        }
    }

    pub fn try_into_string(self) -> Result<Vec<StringRef>, Error> {
        match self {
            Self::StringRef(items) => Ok(items),
            _ => Err(self.casting_error(PropertyType::String)),
        }
    }

    fn casting_error(&self, target: PropertyType) -> Error {
        Error::invalid_property_type(target, self.typ())
    }
}

pub struct NodeMetaStorage<S> {
    storage: Vec<Vec<NodeMeta>>,
    _phantom: PhantomData<S>,
}

impl<S: Schema> NodeMetaStorage<S> {
    pub(crate) fn new() -> Self {
        Self {
            storage: vec![Vec::default(); S::number_of_node_kinds()],
            _phantom: PhantomData,
        }
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        for kind in S::node_kinds() {
            // Safety: both vecs have number_of_node_kinds() slots; kind.index() is in-bounds; separate vecs cannot alias.
            let nodes = unsafe { self.storage.get_unchecked_mut(kind.index()) };
            let other_nodes = unsafe { other.storage.get_unchecked_mut(kind.index()) };
            nodes.append(other_nodes);
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
    pub(crate) fn new() -> Self {
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
            *offsets = StorageArray::new(PropertyType::Int);
            *neighbors = StorageArray::new(PropertyType::NodeRef);
            *properties = StorageArray::new(S::edge_property_type(edge_kind));
        }
        Self {
            storage,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        for (node_kind, direction, edge_kind) in S::edge_storage_slots_iter() {
            let slot = S::edge_storage_slot(node_kind, direction, edge_kind);

            // Safety: storage has edge_storage_size() slots; slot guarantees all three indices are in-bounds and pairwise distinct.
            let [offsets, neighbors, properties] = unsafe {
                self.storage.get_disjoint_unchecked_mut([
                    slot.offset_index(),
                    slot.neighbors_index(),
                    slot.properties_index(),
                ])
            };
            let offsets = offsets.try_as_int_mut().unwrap();

            // Safety: storage has edge_storage_size() slots; same bounds/disjointness as above; separate vec, no aliasing with self.0.
            let [other_offsets, other_neighbors, other_properties] = unsafe {
                other.storage.get_disjoint_unchecked_mut([
                    slot.offset_index(),
                    slot.neighbors_index(),
                    slot.properties_index(),
                ])
            };
            let other_offsets = other_offsets.try_as_int().unwrap();

            let start_offset = offsets.last().copied().unwrap_or(0);
            offsets.reserve(other_offsets.len());

            let start = if offsets.is_empty() { 0 } else { 1 };
            for &offset in &other_offsets[start..] {
                offsets.push(offset + start_offset);
            }

            assert_eq!(neighbors.typ(), other_neighbors.typ());
            neighbors.try_append(other_neighbors).unwrap();

            assert_eq!(properties.typ(), other_properties.typ());
            properties.try_append(other_properties).unwrap();
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
    pub(crate) fn new() -> Self {
        let mut storage = vec![StorageArray::default(); S::property_storage_size()];

        for (node_kind, property_kind) in S::property_storage_slots_iter() {
            let slot = S::property_storage_slot(node_kind, property_kind);

            // Safety: storage has property_storage_size() slots; slot guarantees both indices are in-bounds and distinct.
            let [offsets, values] = unsafe {
                storage.get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
            };
            *offsets = StorageArray::new(PropertyType::Int);
            *values = StorageArray::new(S::node_property_type(property_kind));
        }
        Self {
            storage,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        for (node_kind, property_kind) in S::property_storage_slots_iter() {
            let slot = S::property_storage_slot(node_kind, property_kind);

            // Safety: self.0 has property_storage_size() slots; slot guarantees both indices are in-bounds and distinct.
            let [offsets, values] = unsafe {
                self.storage
                    .get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
            };
            // Panic: The storage is build based on schema, so the properties have right types
            let offsets = offsets.try_as_int_mut().unwrap();

            // Safety: other.0 has property_storage_size() slots; same bounds/disjointness as above; separate vec, no aliasing with self.0.
            let [other_offsets, other_values] = unsafe {
                other
                    .storage
                    .get_disjoint_unchecked_mut([slot.offset_index(), slot.values_index()])
            };
            let other_offsets = other_offsets.try_as_int().unwrap();

            let start_offset = offsets.last().copied().unwrap_or(0);
            offsets.reserve(other_offsets.len());

            let start = if offsets.is_empty() { 0 } else { 1 };
            for &offset in &other_offsets[start..] {
                offsets.push(offset + start_offset);
            }

            assert_eq!(values.typ(), other_values.typ());
            values.try_append(other_values).unwrap();
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
