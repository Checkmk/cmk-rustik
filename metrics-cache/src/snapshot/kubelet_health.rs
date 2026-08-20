use moka::future::Cache;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::kubelet_health::KubeletHealth;

type Ingestion = Arc<MetricsFetcherIngestion<KubeletHealth>>;

/// Kubelet `/healthz` results pushed by metrics-fetcher, keyed by node name.
#[derive(Debug)]
pub(crate) struct KubeletHealths(HashMap<String, Ingestion>);

impl KubeletHealths {
    pub(crate) fn from_cache(cache: &Cache<String, Ingestion>) -> Self {
        Self(
            cache
                .iter()
                .map(|(name, ingestion)| (name.to_string(), ingestion))
                .collect(),
        )
    }

    /// Get the node's snapshotted Kubelet `/healthz` ingestion, if any.
    pub(crate) fn get(&self, node_name: &str) -> Option<&Ingestion> {
        self.0.get(node_name)
    }
}
