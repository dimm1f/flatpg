//! Shared `Schema` fixture used by flatpg's integration tests and benchmarks.
//!
//! Ported from `tests/graph_tests/` so both `tests/*.rs` and `benches/*.rs`
//! (each its own compilation unit) can depend on the same definitions instead
//! of duplicating them.

use flatpg::{prelude::*, schema::Schema};

pub mod graphs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumProperty)]
pub enum Status {
    Active,
    Inactive,
    Banned,
}

#[derive(Debug, Clone, Copy, EnumPropertyRegistry)]
pub enum TestPropEnumsRegistry {
    #[enum_type(Status)]
    Status,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, PropertyItemKind)]
pub enum TestProperty {
    #[property(typ = String, quantity = One)]
    Key,
    #[property(typ = String, quantity = Multi)]
    Values,
    #[property(typ = Int, quantity = One)]
    Count,
    #[property(typ = Enum<Status>, quantity = One)]
    State,
    #[property(typ = Bool, quantity = One)]
    Flag,
    #[property(typ = Byte, quantity = One)]
    Level,
    #[property(typ = Short, quantity = One)]
    Rank,
    #[property(typ = Long, quantity = One)]
    BigCount,
    #[property(typ = Float, quantity = One)]
    Ratio,
    #[property(typ = Double, quantity = One)]
    Score,
    #[property(typ = NodeId, quantity = One)]
    LinkedNode,
    #[property(typ = Enum<Status>, quantity = Multi)]
    Tags,
    #[property(typ = String, quantity = One, rename = Label)]
    Tag,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, NodeItemKind)]
#[node_kind(schema = TestSchema, property_kind = TestProperty)]
pub enum TestNode {
    #[properties(Key, Values, State)]
    Alpha,
    #[properties(Count)]
    Beta,
    #[properties(Flag, Level, Rank, BigCount, Ratio, Score, LinkedNode, Tags)]
    Gamma,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, EdgeItemKind)]
#[edge_kind(schema = TestSchema)]
pub enum TestEdge {
    #[property(typ = None)]
    Plain,
    #[property(typ = String)]
    Labeled,
    #[property(typ = Bool)]
    Active,
    #[property(typ = Byte)]
    Weight,
    #[property(typ = Short)]
    Priority,
    #[property(typ = Int)]
    Distance,
    #[property(typ = Long)]
    Timestamp,
    #[property(typ = Float)]
    Fraction,
    #[property(typ = Double)]
    Precision,
    #[property(typ = NodeId)]
    RefersTo,
    #[property(typ = Enum<Status>)]
    Tagged,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TestSchema;

impl Schema for TestSchema {
    type N = TestNode;
    type E = TestEdge;
    type P = TestProperty;
    type EPR = TestPropEnumsRegistry;

    const NAME: &'static str = "TestSchema";

    const VERSION: flatpg::schema::Version = flatpg::schema::Version::new(1, 0, 0);
}
