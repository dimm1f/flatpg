use core::fmt;
use std::str::FromStr;

use crate::error::Error;
use crate::{EnumPropertyRegistry, ItemAll, ItemAsStr, ItemFromIndex, ItemFromStr, ItemIndex};

/// A self-describing reference to a value of a registered domain enum: which enum type
/// (`enum_property_index`, assigned by a `#[derive(EnumPropertyRegistry)]` registry) and which
/// variant (`variant`, assigned by `#[derive(EnumProperty)]`'s `ItemIndex` on the domain enum
/// itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawEnumId {
    enum_property_index: u16,
    variant: u16,
}

impl RawEnumId {
    pub fn new(enum_property_index: usize, variant: usize) -> Self {
        assert!(enum_property_index <= u16::MAX as usize);
        assert!(variant <= u16::MAX as usize);
        Self {
            enum_property_index: enum_property_index as u16,
            variant: variant as u16,
        }
    }

    pub fn enum_property_index(&self) -> usize {
        self.enum_property_index as usize
    }

    pub fn variant(&self) -> usize {
        self.variant as usize
    }
}

impl fmt::Display for RawEnumId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RawEnumId({},{})",
            self.enum_property_index(),
            self.variant()
        )
    }
}

/// Use when a schema declares no arbitrary-enum properties: `type EPR = NoEnumProps;`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoEnumProps {}

impl ItemAsStr for NoEnumProps {
    fn as_str(&self) -> &'static str {
        match *self {}
    }
}

impl ItemIndex for NoEnumProps {
    fn index(&self) -> usize {
        match *self {}
    }
}

impl ItemFromIndex for NoEnumProps {
    fn from_index(_: usize) -> Option<Self> {
        None
    }
}

impl ItemAll for NoEnumProps {
    fn all() -> &'static [Self] {
        &[]
    }
}

impl FromStr for NoEnumProps {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Err(Self::Err::unknown_label("NoEnumProps", s))
    }
}

impl ItemFromStr for NoEnumProps {}

impl EnumPropertyRegistry for NoEnumProps {
    fn variant_count(&self) -> usize {
        match *self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_enum_id_round_trips_its_fields() {
        let id = RawEnumId::new(3, 42);
        assert_eq!(id.enum_property_index(), 3);
        assert_eq!(id.variant(), 42);
    }

    #[test]
    fn raw_enum_id_display_shows_both_fields() {
        let id = RawEnumId::new(3, 42);
        assert_eq!(id.to_string(), "RawEnumId(3,42)");
    }

    #[test]
    fn no_enum_props_from_index_is_always_none() {
        assert_eq!(NoEnumProps::from_index(0), None);
        assert_eq!(NoEnumProps::from_index(7), None);
    }

    #[test]
    fn no_enum_props_all_is_empty() {
        assert_eq!(NoEnumProps::all(), &[]);
    }

    #[test]
    fn no_enum_props_from_str_always_errors() {
        let err = NoEnumProps::from_str("Anything").unwrap_err();
        assert!(matches!(err, Error::UnknownLabel { enum_name, label }
            if enum_name == "NoEnumProps" && label == "Anything"));
    }
}
