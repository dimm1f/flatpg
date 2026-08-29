use core::fmt;

use crate::{
    EnumPropertyIndex,
    enum_property::RawEnumId,
    error::Error,
    node::{NodeId, RawNodeId},
    schema::Schema,
};

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
    NodeId,
    String,
    Enum,
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
            Self::NodeId => "NodeId",
            Self::String => "String",
            Self::Enum => "Enum",
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
    NodeId(RawNodeId),
    String(String),
    Enum(RawEnumId),
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
            PropertyValue::NodeId(_) => PropertyType::NodeId,
            PropertyValue::String(_) => PropertyType::String,
            PropertyValue::Enum(_) => PropertyType::Enum,
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

impl From<RawNodeId> for PropertyValue {
    fn from(value: RawNodeId) -> Self {
        Self::NodeId(value)
    }
}

impl<S: Schema> From<NodeId<S>> for PropertyValue {
    fn from(value: NodeId<S>) -> Self {
        Self::NodeId(RawNodeId::from(&value))
    }
}

impl<T: EnumPropertyIndex> From<T> for PropertyValue {
    fn from(value: T) -> Self {
        let variant = value.index();
        PropertyValue::Enum(RawEnumId::new(T::enum_property_index(), variant))
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

impl TryFrom<PropertyValue> for RawNodeId {
    type Error = Error;
    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        match value {
            PropertyValue::NodeId(v) => Ok(v),
            other => Err(Error::invalid_property_type(
                PropertyType::NodeId,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_type_display_matches_variant_name() {
        let cases = [
            (PropertyType::None, "None"),
            (PropertyType::Bool, "Bool"),
            (PropertyType::Byte, "Byte"),
            (PropertyType::Short, "Short"),
            (PropertyType::Int, "Int"),
            (PropertyType::Long, "Long"),
            (PropertyType::Float, "Float"),
            (PropertyType::Double, "Double"),
            (PropertyType::NodeId, "NodeId"),
            (PropertyType::String, "String"),
            (PropertyType::Enum, "Enum"),
        ];
        for (typ, name) in cases {
            assert_eq!(typ.to_string(), name);
        }
    }

    #[test]
    fn property_type_default_is_none() {
        assert_eq!(PropertyType::default(), PropertyType::None);
    }

    #[test]
    fn property_value_typ_matches_each_variant() {
        assert_eq!(PropertyValue::Bool(true).typ(), PropertyType::Bool);
        assert_eq!(PropertyValue::Byte(1).typ(), PropertyType::Byte);
        assert_eq!(PropertyValue::Short(1).typ(), PropertyType::Short);
        assert_eq!(PropertyValue::Int(1).typ(), PropertyType::Int);
        assert_eq!(PropertyValue::Long(1).typ(), PropertyType::Long);
        assert_eq!(PropertyValue::Float(1.0).typ(), PropertyType::Float);
        assert_eq!(PropertyValue::Double(1.0).typ(), PropertyType::Double);
        assert_eq!(
            PropertyValue::NodeId(RawNodeId::new(0, 0)).typ(),
            PropertyType::NodeId
        );
        assert_eq!(
            PropertyValue::String("x".into()).typ(),
            PropertyType::String
        );
        assert_eq!(
            PropertyValue::Enum(RawEnumId::new(0, 0)).typ(),
            PropertyType::Enum
        );
    }

    #[test]
    fn from_raw_node_id_wraps_in_node_id_variant() {
        let raw = RawNodeId::new(2, 5);
        assert!(matches!(PropertyValue::from(raw), PropertyValue::NodeId(v) if v == raw));
    }

    #[test]
    fn try_from_property_value_succeeds_for_matching_variant() {
        assert!(bool::try_from(PropertyValue::Bool(true)).unwrap());
        assert_eq!(u8::try_from(PropertyValue::Byte(7)).unwrap(), 7);
        assert_eq!(i16::try_from(PropertyValue::Short(7)).unwrap(), 7);
        assert_eq!(i32::try_from(PropertyValue::Int(7)).unwrap(), 7);
        assert_eq!(i64::try_from(PropertyValue::Long(7)).unwrap(), 7);
        assert_eq!(f32::try_from(PropertyValue::Float(7.0)).unwrap(), 7.0);
        assert_eq!(f64::try_from(PropertyValue::Double(7.0)).unwrap(), 7.0);
        let raw = RawNodeId::new(1, 2);
        assert_eq!(
            RawNodeId::try_from(PropertyValue::NodeId(raw)).unwrap(),
            raw
        );
        assert_eq!(
            String::try_from(PropertyValue::String("hi".into())).unwrap(),
            "hi"
        );
    }

    #[test]
    fn try_from_property_value_fails_for_mismatched_variant() {
        let mismatched = PropertyValue::String("wrong type".into());

        let err = bool::try_from(mismatched.clone()).unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidPropertyType { expected, found }
                if expected == "Bool" && found == "String"
        ));

        assert!(u8::try_from(mismatched.clone()).is_err());
        assert!(i16::try_from(mismatched.clone()).is_err());
        assert!(i32::try_from(mismatched.clone()).is_err());
        assert!(i64::try_from(mismatched.clone()).is_err());
        assert!(f32::try_from(mismatched.clone()).is_err());
        assert!(f64::try_from(mismatched.clone()).is_err());
        assert!(RawNodeId::try_from(mismatched.clone()).is_err());
        assert!(String::try_from(PropertyValue::Bool(true)).is_err());
    }
}
