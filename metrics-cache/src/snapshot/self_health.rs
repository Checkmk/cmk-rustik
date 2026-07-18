use k8s_openapi::api::core::v1::Node;
use moka::future::Cache;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::kubelet_stats::StatsSummary;

/// A point-in-time view of the health of cmk-rustik and its components:
/// metrics-cache and metrics-fetcher.
///
/// This is included in the [`crate::snapshot::Snapshot`] and used to generate
/// cluster-level sections for alerting about the state of rustik, such as when
/// a node last reported in (metrics-fetcher), or if the reflectors had errors.
///
/// When constructing this (with [`SelfHealth::new()`]), you must supply a slice
/// of [`Arc<Node>`]s. This is because we need a source of truth that is _not_
/// the cache data for determining which nodes _should_ be present. The cache
/// data can come and go at any time (expiring TTLs, for example). And nodes can
/// come and go as they join and leave a cluster. We need to account for these
/// situations somehow: If we relied only on the cache data, and a node's TTL
/// expired, we would not know if the node went offline due to a problem or if
/// it were removed from the cluster and now expected to be absent.
///
/// The cluster API _knows_ if a node should be present or not. So we use that
/// as the source of truth. Then in the monitoring/Checkmk side, we can alert on
/// "Kubernetes said the node should be there, it's missing from the cache data,
/// that means it stopped reporting at some point, CRIT!"
#[derive(Debug)]
pub struct SelfHealth {
    /// Maps node names to the age of the last kubelet stats push from the node.
    pub kubelet_stats_summary_age: BTreeMap<String, Option<Duration>>,
}

impl SelfHealth {
    pub fn new(
        now: Instant,
        nodes: &[Arc<Node>],
        kubelet_stats_summary_cache: &Cache<String, Arc<MetricsFetcherIngestion<StatsSummary>>>,
    ) -> SelfHealth {
        let kubelet_stats_summary_age = Self::cache_age(now, nodes, kubelet_stats_summary_cache);
        SelfHealth {
            kubelet_stats_summary_age,
        }
    }

    /// Given an `Instant` representing _the moment the `Snapshot` is being
    /// taken_, a collection of nodes, and a cache of metrics-fetcher ingestions
    /// from nodes, calculate how long it has been since the node reported in
    /// and collect the results, keyed on the node name.
    ///
    /// The collection of nodes is taken as the source of truth for "which nodes
    /// _are supposed to be_ present?" and anything that is not in the cache but
    /// is in the collection of nodes is returned as `None` in the resulting map
    /// with the expectation that monitoring reports the node as having stopped
    /// reporting its metrics.
    fn cache_age<T>(
        now: Instant,
        nodes: &[Arc<Node>],
        cache: impl IntoIterator<Item = (Arc<String>, Arc<MetricsFetcherIngestion<T>>)>,
    ) -> BTreeMap<String, Option<Duration>> {
        let intermediary: HashMap<String, Duration> = cache
            .into_iter()
            .map(|(name, ingestion)| (name.to_string(), now - ingestion.received_at))
            .collect();

        let mut map = BTreeMap::new();
        for node in nodes {
            let Some(node_name) = node.metadata.name.clone() else {
                continue;
            };
            let since_update = intermediary.get(&node_name).copied();
            map.insert(node_name, since_update);
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::*;

    fn ingestion(instant: Instant) -> Arc<MetricsFetcherIngestion<()>> {
        MetricsFetcherIngestion {
            received_at: instant,
            payload: (),
        }
        .into()
    }

    #[test]
    fn cache_age() {
        let now = Instant::now();
        let nodes = &[
            node("node1334").into(),
            node("node1335").into(),
            node("node1336").into(),
            node("node1337").into(),
        ];
        let cache = [
            (
                s("node1334").into(),
                ingestion(now - Duration::from_mins(5)),
            ),
            (
                s("node1335").into(),
                ingestion(now - Duration::from_mins(3)),
            ),
            (
                s("node1336").into(),
                ingestion(now - Duration::from_secs(26)),
            ),
            (
                s("decommissioned01").into(),
                ingestion(now - Duration::from_mins(29)),
            ),
        ];
        let ages = SelfHealth::cache_age(now, nodes, cache);

        assert_eq!(ages.len(), nodes.len());
        assert_eq!(ages["node1334"].unwrap(), Duration::from_mins(5));
        assert_eq!(ages["node1335"].unwrap(), Duration::from_mins(3));
        assert_eq!(ages["node1336"].unwrap(), Duration::from_secs(26));

        // Node in nodes store but not in cache gets added to the result as None
        assert!(ages["node1337"].is_none());

        // A node in the cache but not in the nodes store is not added
        assert!(!ages.contains_key("decommissioned01"));
    }
}
