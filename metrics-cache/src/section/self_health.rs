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
fn age_map_secs<S: Serializer>(
    map: &&BTreeMap<String, Option<Duration>>,
    ser: S,
) -> Result<S::Ok, S::Error> {
    ser.collect_map(map.iter().map(|(k, v)| (k, v.map(|d| d.as_secs_f64()))))
}

fn duration_to_secs<S: Serializer>(duration: &Option<Duration>, ser: S) -> Result<S::Ok, S::Error> {
    duration.map(|d| d.as_secs_f64()).serialize(ser)
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
pub(crate) struct KubeRustikHealthV1<'a> {
    #[serde(serialize_with = "age_map_secs")]
    node_kubelet_stats_last_heard_secs: &'a BTreeMap<String, Option<Duration>>,
    reflector_healths: BTreeMap<&'static str, ReflectorHealth>,
}

impl<'a> KubeRustikHealthV1<'a> {
    pub fn from_self_health(self_health: &'a SelfHealth) -> KubeRustikHealthV1<'a> {
        let reflector_healths = self_health
            .reflector_healths
            .iter()
            .map(|(k, v)| (*k, ReflectorHealth::from(v)))
            .collect();

        KubeRustikHealthV1 {
            node_kubelet_stats_last_heard_secs: &self_health.kubelet_stats_summary_age,
            reflector_healths,
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
    use crate::test_support::*;

    #[test]
    fn kube_rustik_health_v1() {
        let kubelet_stats_summary_age = BTreeMap::from([
            (s("node01"), Some(Duration::from_secs(26))),
            (s("node02"), Some(Duration::from_mins(4))),
            (s("node03"), Some(Duration::from_mins(2))),
            (s("offline01"), None),
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
        let self_health = SelfHealth {
            kubelet_stats_summary_age,
            reflector_healths,
        };
        let section = KubeRustikHealthV1::from_self_health(&self_health);
        insta::assert_json_snapshot!(section);
    }
}
