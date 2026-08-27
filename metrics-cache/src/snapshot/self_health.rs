use k8s_openapi::api::apps::v1::DaemonSet;
use moka::future::Cache;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::kubelet_health::KubeletHealth;
use crate::ingest::kubelet_stats::StatsSummary;
use crate::ingest::reflectors::{self, FrozenReflectorHealths};
use crate::ingest::system_agent::SystemAgentOutput;
use crate::snapshot::owner_graph::OwnerGraph;

/// A point-in-time view of the health of cmk-rustik and its components:
/// metrics-cache and metrics-fetcher.
///
/// This is included in the [`crate::snapshot::Snapshot`] and used to generate
/// cluster-level sections for alerting about the state of rustik, such as when
/// a node last reported in (metrics-fetcher), or if the reflectors had errors.
///
/// Scheduled Pods belonging to the metrics-fetcher DaemonSet are the source of
/// truth for which nodes should report. This accounts for nodes on which the
/// DaemonSet is intentionally not scheduled while still retaining missing
/// cache entries for broken metrics-fetcher Pods.
#[derive(Debug, Default)]
pub struct SelfHealth {
    /// Health data for each of the payloads sent by metrics-fetcher, keyed by
    /// node name.
    pub node_metrics_fetchers: BTreeMap<String, NodeMetricsFetcherHealth>,
    /// Maps kind names to reflector states.
    pub(crate) reflector_healths: BTreeMap<&'static str, ReflectorHealth>,
}

/// Self-health data for the metrics-fetcher running on a single node.
#[derive(Debug, Default)]
pub struct NodeMetricsFetcherHealth {
    pub kubelet_stats: MetricsFetcherIngestionHealth,
    pub kubelet_health: MetricsFetcherIngestionHealth,
    pub system_agent: MetricsFetcherIngestionHealth,
}

/// Freshness and metadata for one kind of metrics-fetcher ingestion.
#[derive(Debug, Default)]
pub struct MetricsFetcherIngestionHealth {
    pub last_heard_age: Option<Duration>,
    pub scrape_time: Option<Duration>,
    pub version: Option<String>,
}

/// Identifies the metrics-fetcher DaemonSet in the reflector store.
#[derive(Clone, Debug)]
pub struct MetricsFetcherDaemonSet {
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Default)]
pub(crate) struct ReflectorHealth {
    pub(crate) has_been_initialized: bool,
    pub(crate) relist_started_age: Option<Duration>,
    pub(crate) relist_completed_age: Option<Duration>,
    pub(crate) relist_duration: Option<Duration>,
    pub(crate) last_error_age: Option<Duration>,
    pub(crate) errors_total: u64,
}

impl ReflectorHealth {
    fn from_reflector_health(health: reflectors::ReflectorHealth, now: Instant) -> Self {
        Self {
            has_been_initialized: health.has_been_initialized,
            relist_started_age: health
                .relist_started_at
                .map(|i| now.saturating_duration_since(i)),
            relist_completed_age: health
                .relist_completed_at
                .map(|i| now.saturating_duration_since(i)),
            relist_duration: health.relist_duration,
            last_error_age: health
                .last_error_at
                .map(|i| now.saturating_duration_since(i)),
            errors_total: health.errors_total,
        }
    }
}

impl SelfHealth {
    pub(crate) fn new(
        now: Instant,
        expected_nodes: &BTreeSet<String>,
        reflector_healths: FrozenReflectorHealths,
        kubelet_stats_summary_cache: &Cache<String, Arc<MetricsFetcherIngestion<StatsSummary>>>,
        kubelet_health_cache: &Cache<String, Arc<MetricsFetcherIngestion<KubeletHealth>>>,
        system_agent_cache: &Cache<String, Arc<MetricsFetcherIngestion<SystemAgentOutput>>>,
    ) -> SelfHealth {
        let kubelet_stats = Self::cache_health(now, expected_nodes, kubelet_stats_summary_cache);
        let mut kubelet_health = Self::cache_health(now, expected_nodes, kubelet_health_cache);
        let mut system_agent = Self::cache_health(now, expected_nodes, system_agent_cache);
        let node_metrics_fetchers = kubelet_stats
            .into_iter()
            .map(|(node, kubelet_stats)| {
                let kubelet_health = kubelet_health.remove(&node).unwrap_or_default();
                let system_agent = system_agent.remove(&node).unwrap_or_default();
                (
                    node,
                    NodeMetricsFetcherHealth {
                        kubelet_stats,
                        kubelet_health,
                        system_agent,
                    },
                )
            })
            .collect();
        let reflector_healths = Self::reflector_healths_from_frozen(now, reflector_healths);
        SelfHealth {
            node_metrics_fetchers,
            reflector_healths,
        }
    }

    pub(crate) fn expected_node_names(
        daemonsets: &[Arc<DaemonSet>],
        owner_graph: &OwnerGraph,
        identity: Option<&MetricsFetcherDaemonSet>,
    ) -> BTreeSet<String> {
        let Some(identity) = identity else {
            return BTreeSet::new();
        };
        let Some(uid) = daemonsets
            .iter()
            .find(|daemonset| {
                daemonset.metadata.namespace.as_deref() == Some(&identity.namespace)
                    && daemonset.metadata.name.as_deref() == Some(&identity.name)
            })
            .and_then(|daemonset| daemonset.metadata.uid.as_deref())
        else {
            return BTreeSet::new();
        };

        owner_graph
            .pods_by_controller(uid)
            .iter()
            .filter_map(|pod| pod.spec.as_ref()?.node_name.clone())
            .collect()
    }

    /// Given an `Instant` representing _the moment the `Snapshot` is being
    /// taken_, a collection of expected node names, and a cache of
    /// metrics-fetcher ingestions, calculate how long it has been since each
    /// node reported in and collect the results, keyed on the node name.
    ///
    /// The node names are taken as the source of truth for "which nodes are
    /// supposed to be present?" Anything that is not in the cache but is in
    /// this collection is returned with empty health data, with
    /// the expectation that monitoring reports the node as having stopped
    /// reporting its metrics.
    fn cache_health<T>(
        now: Instant,
        nodes: &BTreeSet<String>,
        cache: impl IntoIterator<Item = (Arc<String>, Arc<MetricsFetcherIngestion<T>>)>,
    ) -> BTreeMap<String, MetricsFetcherIngestionHealth> {
        let mut intermediary: HashMap<String, MetricsFetcherIngestionHealth> = cache
            .into_iter()
            .map(|(name, ingestion)| {
                (
                    name.to_string(),
                    MetricsFetcherIngestionHealth {
                        last_heard_age: Some(now.saturating_duration_since(ingestion.received_at)),
                        scrape_time: ingestion.metadata.scrape_time,
                        version: ingestion.metadata.version.clone(),
                    },
                )
            })
            .collect();

        let mut map = BTreeMap::new();
        for node_name in nodes {
            let health = intermediary.remove(node_name).unwrap_or_default();
            map.insert(node_name.clone(), health);
        }
        map
    }

    /// Given an `Instant` representing _the moment the `Snapshot` is being
    /// taken_, and a [`crate::ingest::reflectors::FrozenReflectorHealths`],
    /// generate a `BTreeMap` using the `IntoIterator` instance of the
    /// `FrozenReflectorHealths`, converting each reflector health into a
    /// [`crate::snapshot::self_health::ReflectorHealth`].
    fn reflector_healths_from_frozen(
        now: Instant,
        healths: FrozenReflectorHealths,
    ) -> BTreeMap<&'static str, ReflectorHealth> {
        healths
            .into_iter()
            .map(|(kind, health)| (kind, ReflectorHealth::from_reflector_health(health, now)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ingest::MetricsFetcherMetadata;
    use crate::test_support::*;

    fn ingestion(instant: Instant) -> Arc<MetricsFetcherIngestion<()>> {
        MetricsFetcherIngestion {
            received_at: instant,
            metadata: MetricsFetcherMetadata::default(),
            payload: (),
        }
        .into()
    }

    #[test]
    fn cache_health() {
        let now = Instant::now();
        let nodes =
            BTreeSet::from(["node1334", "node1335", "node1336", "node1337"].map(str::to_string));
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
        let healths = SelfHealth::cache_health(now, &nodes, cache);

        assert_eq!(healths.len(), nodes.len());
        assert_eq!(
            healths["node1334"].last_heard_age,
            Some(Duration::from_mins(5))
        );
        assert_eq!(
            healths["node1335"].last_heard_age,
            Some(Duration::from_mins(3))
        );
        assert_eq!(
            healths["node1336"].last_heard_age,
            Some(Duration::from_secs(26))
        );

        // Expected node not in cache gets added to the result as None
        assert!(healths["node1337"].last_heard_age.is_none());

        // A node in the cache but not in the expected set is not added
        assert!(!healths.contains_key("decommissioned01"));
    }

    #[test]
    fn expected_nodes_are_scheduled_metrics_fetcher_pods() {
        let mut daemonset = daemonset("metrics-fetcher");
        daemonset.metadata.uid = Some(s("daemonset-uid"));
        let owner_graph = OwnerGraph {
            owner_ref_by_uid: HashMap::new(),
            pods_by_controller: HashMap::from([(
                "daemonset-uid".into(),
                vec![
                    Arc::new(pod("fetcher", Some("linux-node"))),
                    Arc::new(pod("pending-fetcher", None)),
                ],
            )]),
            jobs_by_controller: HashMap::new(),
        };
        let identity = MetricsFetcherDaemonSet {
            namespace: s("the-namespace-of-all-namespaces"),
            name: s("metrics-fetcher"),
        };

        let nodes =
            SelfHealth::expected_node_names(&[Arc::new(daemonset)], &owner_graph, Some(&identity));

        assert_eq!(nodes, BTreeSet::from([s("linux-node")]));
    }

    #[test]
    fn cache_health_preserves_ingestion_metadata() {
        let now = Instant::now();
        let metadata = MetricsFetcherMetadata {
            scrape_time: Some(Duration::from_millis(945)),
            version: Some(s("1.1000.0")),
        };
        let ingestion = Arc::new(MetricsFetcherIngestion {
            received_at: now - Duration::from_secs(12),
            metadata,
            payload: (),
        });

        let health = SelfHealth::cache_health(
            now,
            &BTreeSet::from([s("node01")]),
            [(s("node01").into(), ingestion)],
        );

        assert_eq!(
            health["node01"].last_heard_age,
            Some(Duration::from_secs(12))
        );
        assert_eq!(
            health["node01"].scrape_time,
            Some(Duration::from_millis(945))
        );
        assert_eq!(health["node01"].version.as_deref(), Some("1.1000.0"));
    }

    #[test]
    fn from_reflector_health() {
        let now = Instant::now();
        let health = reflectors::ReflectorHealth {
            has_been_initialized: true,
            relist_started_at: Some(now - Duration::from_secs(10)),
            relist_completed_at: Some(now - Duration::from_secs(20)),
            relist_duration: Some(Duration::from_secs(3)),
            last_error_at: Some(now - Duration::from_secs(30)),
            errors_total: 7,
        };
        let converted = ReflectorHealth::from_reflector_health(health, now);
        assert!(converted.has_been_initialized);
        assert_eq!(converted.relist_started_age, Some(Duration::from_secs(10)));
        assert_eq!(
            converted.relist_completed_age,
            Some(Duration::from_secs(20))
        );
        assert_eq!(converted.relist_duration, Some(Duration::from_secs(3)));
        assert_eq!(converted.last_error_age, Some(Duration::from_secs(30)));
        assert_eq!(converted.errors_total, 7);
    }

    #[test]
    fn from_reflector_health_defaults() {
        let now = Instant::now();
        let health = reflectors::ReflectorHealth::default();
        let converted = ReflectorHealth::from_reflector_health(health, now);
        assert!(!converted.has_been_initialized);
        assert!(converted.relist_started_age.is_none());
        assert!(converted.relist_completed_age.is_none());
        assert!(converted.relist_duration.is_none());
        assert!(converted.last_error_age.is_none());
        assert_eq!(converted.errors_total, 0);
    }

    #[test]
    fn from_reflector_health_future_time_does_not_panic() {
        let now = Instant::now();
        let health = reflectors::ReflectorHealth {
            last_error_at: Some(now + Duration::from_secs(1)),
            ..Default::default()
        };
        let converted = ReflectorHealth::from_reflector_health(health, now);
        assert_eq!(converted.last_error_age, Some(Duration::ZERO));
        assert_eq!(converted.errors_total, 0);
    }

    #[test]
    fn reflector_healths_from_frozen_sanity() {
        let now = Instant::now();
        let frozen_healths = FrozenReflectorHealths::default();
        let map = SelfHealth::reflector_healths_from_frozen(now, frozen_healths);
        assert!(map.contains_key("ReplicaSet"));
    }
}
