//! Raw, unchecked access to a graph's flat CSR storage, for bulk loading, deserialization,
//! or other lower-level construction that doesn't go through [`crate::graph::builder::GraphDiff`].
//!
//! Converting a [`RawGraph<S>`] back into a [`Graph<S>`] runs the same checks as
//! [`CheckIntegrity::check_integrity`] before handing back a checked graph — see
//! [`crate::graph::integrity`] for what it verifies and its known limitations.

use crate::{
    error::Error,
    graph::{
        Graph,
        integrity::{CheckIntegrity, check_integrity},
    },
    schema::Schema,
    storage::{EdgeStorage, NodeMetaStorage, PropertyStorage},
    strings_pool::StringsPool,
};

pub struct RawGraph<S> {
    pub node_meta_storage: NodeMetaStorage<S>,
    pub edge_storage: EdgeStorage<S>,
    pub property_storage: PropertyStorage<S>,
    pub strings: StringsPool,
}

impl<S: Schema> RawGraph<S> {
    pub fn new() -> Self {
        Self {
            node_meta_storage: NodeMetaStorage::new(),
            edge_storage: EdgeStorage::new(),
            property_storage: PropertyStorage::new(),
            strings: StringsPool::new(),
        }
    }
}

impl<S: Schema> Default for RawGraph<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Schema> From<Graph<S>> for RawGraph<S> {
    fn from(graph: Graph<S>) -> Self {
        Self {
            node_meta_storage: graph.node_meta_storage,
            edge_storage: graph.edge_storage,
            property_storage: graph.property_storage,
            strings: graph.strings,
        }
    }
}

impl<S: Schema> CheckIntegrity<S> for RawGraph<S> {
    fn check_integrity(&self) -> Result<(), Error> {
        check_integrity(
            &self.node_meta_storage,
            &self.edge_storage,
            &self.property_storage,
            &self.strings,
        )
    }
}

impl<S: Schema> TryFrom<RawGraph<S>> for Graph<S> {
    type Error = Error;

    fn try_from(raw: RawGraph<S>) -> Result<Self, Self::Error> {
        check_integrity(
            &raw.node_meta_storage,
            &raw.edge_storage,
            &raw.property_storage,
            &raw.strings,
        )?;
        Ok(Graph {
            node_meta_storage: raw.node_meta_storage,
            edge_storage: raw.edge_storage,
            property_storage: raw.property_storage,
            strings: raw.strings,
        })
    }
}
