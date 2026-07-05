use core::fmt;

use crate::{error::Error, node::NodeRef};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantityType {
    One,
    Multi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PropertyType {
    #[default]
    None,
    Bool,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    NodeRef,
    String,
}

impl fmt::Display for PropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::None => "None",
            Self::Bool => "Bool",
            Self::Byte => "Byte",
            Self::Short => "Short",
            Self::Int => "Int",
            Self::Long => "Long",
            Self::Float => "Float",
            Self::Double => "Double",
            Self::NodeRef => "NodeRef",
            Self::String => "String",
        };
        write!(f, "{}", name)
    }
}

#[derive(Debug, Clone)]
pub enum PropertyValue {
    Bool(bool),
    Byte(u8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    NodeRef(NodeRef),
    String(String),
}

impl PropertyValue {
    pub fn typ(&self) -> PropertyType {
        match self {
            PropertyValue::Bool(_) => PropertyType::Bool,
            PropertyValue::Byte(_) => PropertyType::Byte,
            PropertyValue::Short(_) => PropertyType::Short,
            PropertyValue::Int(_) => PropertyType::Int,
            PropertyValue::Long(_) => PropertyType::Long,
            PropertyValue::Float(_) => PropertyType::Float,
            PropertyValue::Double(_) => PropertyType::Double,
            PropertyValue::NodeRef(_) => PropertyType::NodeRef,
            PropertyValue::String(_) => PropertyType::String,
        }
    }
}

impl From<bool> for PropertyValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<u8> for PropertyValue {
    fn from(value: u8) -> Self {
        Self::Byte(value)
    }
}

impl From<i16> for PropertyValue {
    fn from(value: i16) -> Self {
        Self::Short(value)
    }
}

impl From<i32> for PropertyValue {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Long(value)
    }
}

impl From<f32> for PropertyValue {
    fn from(value: f32) -> Self {
        Self::Float(value)
    }
}

impl From<f64> for PropertyValue {
    fn from(value: f64) -> Self {
        Self::Double(value)
    }
}

impl From<NodeRef> for PropertyValue {
    fn from(value: NodeRef) -> Self {
        Self::NodeRef(value)
    }
}

impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl TryFrom<PropertyValue> for bool {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Bool(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::Bool,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for u8 {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Byte(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::Byte,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for i16 {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Short(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::Short,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for i32 {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Int(v) => Ok(v),
            other => Err(Error::invalid_property_type(PropertyType::Int, other.typ())),
        }
    }
}

impl TryFrom<PropertyValue> for i64 {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Long(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::Long,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for f32 {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Float(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::Float,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for f64 {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::Double(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::Double,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for NodeRef {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::NodeRef(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::NodeRef,
                other.typ(),
            )),
        }
    }
}

impl TryFrom<PropertyValue> for String {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::String(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::String,
                other.typ(),
            )),
        }
    }
}
