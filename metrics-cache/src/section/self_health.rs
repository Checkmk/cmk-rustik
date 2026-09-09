//! Self-health section(s) for rustik.
//!
//! The sections here provide information about the current health of rustik
//! and its two components (metrics-cache and metrics-fetcher).
//!
//! These get emitted on the _cluster_ piggyback host.

use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::time::Duration;

use crate::section::Section;
use crate::snapshot::self_health::SelfHealth;

/// By default serde serializes Duration as a dict of time units (mins, secs,
/// etc.). This is less useful for parsing later on, we want pure seconds, so
/// this is used to teach serde how to serialize a cache map from the snapshot's
/// [`SelfHealth`] in seconds.
fn duration_to_secs<S: Serializer>(duration: &Option<Duration>, ser: S) -> Result<S::Ok, S::Error> {
    duration.map(|d| d.as_secs_f64()).serialize(ser)
}

#[derive(Debug, Serialize)]
struct MetricsFetcherIngestionHealth<'a> {
    #[serde(serialize_with = "duration_to_secs")]
    last_heard_age_secs: Option<Duration>,
    #[serde(serialize_with = "duration_to_secs")]
    scrape_time_secs: Option<Duration>,
    version: Option<&'a str>,
    git_sha: Option<&'a str>,
}

impl<'a> From<&'a crate::snapshot::self_health::MetricsFetcherIngestionHealth>
    for MetricsFetcherIngestionHealth<'a>
{
    fn from(value: &'a crate::snapshot::self_health::MetricsFetcherIngestionHealth) -> Self {
        Self {
            last_heard_age_secs: value.last_heard_age,
            scrape_time_secs: value.scrape_time,
            version: value.version.as_deref(),
            git_sha: value.git_sha.as_deref(),
        }
    }
}

#[derive(Debug, Serialize)]
struct NodeMetricsFetcherHealth<'a> {
    kubelet_stats: MetricsFetcherIngestionHealth<'a>,
    kubelet_health: MetricsFetcherIngestionHealth<'a>,
    system_agent: MetricsFetcherIngestionHealth<'a>,
}

impl<'a> From<&'a crate::snapshot::self_health::NodeMetricsFetcherHealth>
    for NodeMetricsFetcherHealth<'a>
{
    fn from(value: &'a crate::snapshot::self_health::NodeMetricsFetcherHealth) -> Self {
        Self {
            kubelet_stats: MetricsFetcherIngestionHealth::from(&value.kubelet_stats),
            kubelet_health: MetricsFetcherIngestionHealth::from(&value.kubelet_health),
            system_agent: MetricsFetcherIngestionHealth::from(&value.system_agent),
        }
    }
}

#[derive(Debug, Serialize)]
struct ReflectorHealth {
    has_been_initialized: bool,
    #[serde(serialize_with = "duration_to_secs")]
    relist_started_age_secs: Option<Duration>,
    #[serde(serialize_with = "duration_to_secs")]
    relist_completed_age_secs: Option<Duration>,
    #[serde(serialize_with = "duration_to_secs")]
    relist_duration_secs: Option<Duration>,
    #[serde(serialize_with = "duration_to_secs")]
    last_error_age_secs: Option<Duration>,
    errors_total: u64,
}

impl From<&crate::snapshot::self_health::ReflectorHealth> for ReflectorHealth {
    fn from(value: &crate::snapshot::self_health::ReflectorHealth) -> Self {
        Self {
            has_been_initialized: value.has_been_initialized,
            relist_started_age_secs: value.relist_started_age,
            relist_completed_age_secs: value.relist_completed_age,
            relist_duration_secs: value.relist_duration,
            last_error_age_secs: value.last_error_age,
            errors_total: value.errors_total,
        }
    }
}

#[derive(Debug, Serialize)]
struct MetricsCacheMetadata<'a> {
    version: &'a str,
    git_sha: Option<&'a str>,
}

impl<'a> MetricsCacheMetadata<'a> {
    fn new(version: &'a str, git_sha: Option<&'a str>) -> Self {
        Self { version, git_sha }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct KubeRustikHealthV1<'a> {
    metrics_fetchers: BTreeMap<&'a str, NodeMetricsFetcherHealth<'a>>,
    reflector_healths: BTreeMap<&'static str, ReflectorHealth>,
    metrics_cache: MetricsCacheMetadata<'a>,
}

impl<'a> KubeRustikHealthV1<'a> {
    pub fn from_self_health(self_health: &'a SelfHealth) -> KubeRustikHealthV1<'a> {
        let reflector_healths = self_health
            .reflector_healths
            .iter()
            .map(|(k, v)| (*k, ReflectorHealth::from(v)))
            .collect();
        let metrics_fetchers = self_health
            .node_metrics_fetchers
            .iter()
            .map(|(node, health)| (node.as_str(), NodeMetricsFetcherHealth::from(health)))
            .collect();

        KubeRustikHealthV1 {
            metrics_fetchers,
            reflector_healths,
            metrics_cache: MetricsCacheMetadata::new(
                self_health.metrics_cache_build_info.version,
                self_health.metrics_cache_build_info.git_sha,
            ),
        }
    }
}

impl Section for KubeRustikHealthV1<'_> {
    const NAME: &'static str = "kube_rustik_health_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::snapshot;
    use crate::snapshot::self_health::MetricsCacheBuildInfo;
    use crate::test_support::*;

    #[test]
    fn kube_rustik_health_v1() {
        let node_metrics_fetchers = BTreeMap::from([
            (
                s("node01"),
                snapshot::self_health::NodeMetricsFetcherHealth {
                    kubelet_stats: snapshot::self_health::MetricsFetcherIngestionHealth {
                        last_heard_age: Some(Duration::from_secs(26)),
                        scrape_time: Some(Duration::from_millis(150)),
                        version: Some(s("1.1000.0")),
                        git_sha: Some(s("39ead942a6da3e42c8b0fb37810eca12344a56c7")),
                    },
                    kubelet_health: snapshot::self_health::MetricsFetcherIngestionHealth {
                        last_heard_age: Some(Duration::from_secs(24)),
                        scrape_time: Some(Duration::from_millis(45)),
                        version: Some(s("1.1000.0")),
                        git_sha: Some(s("39ead942a6da3e42c8b0fb37810eca12344a56c7")),
                    },
                    system_agent: snapshot::self_health::MetricsFetcherIngestionHealth {
                        last_heard_age: None,
                        scrape_time: None,
                        version: None,
                        git_sha: None,
                    },
                },
            ),
            (
                s("offline01"),
                snapshot::self_health::NodeMetricsFetcherHealth::default(),
            ),
        ]);
        let reflector_healths = BTreeMap::from([
            ("Pod", snapshot::self_health::ReflectorHealth::default()),
            (
                "ReplicaSet",
                snapshot::self_health::ReflectorHealth::default(),
            ),
            (
                "DaemonSet",
                snapshot::self_health::ReflectorHealth::default(),
            ),
        ]);
        let metrics_cache_build_info = MetricsCacheBuildInfo {
            version: "1.1000.1",
            git_sha: Some("40abc123a6da3e42c8b0fb37810eca54321a56a1"),
        };
        let self_health = SelfHealth {
            node_metrics_fetchers,
            reflector_healths,
            metrics_cache_build_info,
        };
        let section = KubeRustikHealthV1::from_self_health(&self_health);
        insta::assert_json_snapshot!(section);
    }
}
