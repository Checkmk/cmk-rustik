use moka::future::Cache;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ingest::{MetricsFetcherIngestion, SystemAgentOutput};

type Ingestion = Arc<MetricsFetcherIngestion<SystemAgentOutput>>;

/// Checkmk system agent output, keyed by node name.
#[derive(Debug)]
pub(crate) struct SystemAgentOutputs(HashMap<String, Ingestion>);

impl SystemAgentOutputs {
    pub(crate) fn from_cache(cache: &Cache<String, Ingestion>) -> Self {
        Self(
            cache
                .iter()
                .map(|(name, ingestion)| (name.to_string(), ingestion))
                .collect(),
        )
    }

    /// Get the node's snapshotted Checkmk system agent output, if any.
    pub(crate) fn get(&self, node_name: &str) -> Option<&Ingestion> {
        self.0.get(node_name)
    }
}
