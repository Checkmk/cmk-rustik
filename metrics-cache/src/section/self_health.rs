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
    ser.collect_map(map.iter().map(|(k, v)| (k, v.map(|d| d.as_secs()))))
}

#[derive(Debug, Serialize)]
pub(crate) struct KubeRustikHealthV1<'a> {
    #[serde(serialize_with = "age_map_secs")]
    node_kubelet_stats_last_heard_secs: &'a BTreeMap<String, Option<Duration>>,
}

impl<'a> KubeRustikHealthV1<'a> {
    pub fn from_self_health(self_health: &'a SelfHealth) -> KubeRustikHealthV1<'a> {
        KubeRustikHealthV1 {
            node_kubelet_stats_last_heard_secs: &self_health.kubelet_stats_summary_age,
        }
    }
}

impl Section for KubeRustikHealthV1<'_> {
    const NAME: &'static str = "kube_rustik_health_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::*;

    #[test]
    fn kube_rustik_health_v1() {
        let kubelet_stats_summary_age = BTreeMap::from([
            (s("node01"), Some(Duration::from_secs(26))),
            (s("node02"), Some(Duration::from_mins(4))),
            (s("node03"), Some(Duration::from_mins(2))),
            (s("offline01"), None),
        ]);
        let self_health = SelfHealth {
            kubelet_stats_summary_age,
        };
        let section = KubeRustikHealthV1::from_self_health(&self_health);
        insta::assert_json_snapshot!(section);
    }
}
