use k8s_openapi::jiff::Timestamp;
use serde::Deserialize;
use std::collections::HashMap;

/// The parsed response from the Kubelet endpoint `/stats/summary`.
#[derive(Clone, Debug, Deserialize)]
pub struct StatsSummary {
    pub node: Node,
    pub pods: Vec<Pod>,
}

impl StatsSummary {
    /// Replace kubelet's short-window CPU rates with rates calculated over the
    /// interval between this summary and the previous one.
    pub(crate) fn with_cpu_rates_from(mut self, previous: Option<&Self>) -> Self {
        let previous_containers: HashMap<(&str, &str), &Container> = previous
            .into_iter()
            .flat_map(|summary| &summary.pods)
            .flat_map(|pod| {
                pod.containers.iter().map(|container| {
                    (
                        (pod.pod_ref.uid.as_str(), container.name.as_str()),
                        container,
                    )
                })
            })
            .collect();

        for pod in &mut self.pods {
            for container in &mut pod.containers {
                let rate = previous_containers
                    .get(&(pod.pod_ref.uid.as_str(), container.name.as_str()))
                    .and_then(|previous| container.cpu_rate_since(previous));

                if let Some(cpu) = &mut container.cpu {
                    cpu.usage_nano_cores = rate;
                }
            }
        }

        self
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Node {
    #[serde(rename = "nodeName")]
    pub node_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Pod {
    #[serde(rename = "podRef")]
    pub pod_ref: PodReference,
    pub containers: Vec<Container>,
    pub volume: Option<Vec<Volume>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PodReference {
    pub name: String,
    pub namespace: String,
    pub uid: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Container {
    pub name: String,
    #[serde(rename = "startTime")]
    pub start_time: Option<Timestamp>,
    pub cpu: Option<CPUStats>,
    pub memory: Option<MemoryStats>,
    pub swap: Option<SwapStats>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CPUStats {
    pub time: Option<Timestamp>,
    #[serde(rename = "usageNanoCores")]
    pub usage_nano_cores: Option<u64>,
    #[serde(rename = "usageCoreNanoSeconds")]
    pub usage_core_nano_seconds: Option<u64>,
}

impl Container {
    fn cpu_rate_since(&self, previous: &Self) -> Option<u64> {
        if self.start_time.as_ref()? != previous.start_time.as_ref()? {
            return None;
        }
        self.cpu.as_ref()?.rate_since(previous.cpu.as_ref()?)
    }
}

impl CPUStats {
    fn rate_since(&self, previous: &Self) -> Option<u64> {
        let elapsed_ms =
            self.time.as_ref()?.as_millisecond() - previous.time.as_ref()?.as_millisecond();
        let used_ns = self
            .usage_core_nano_seconds?
            .checked_sub(previous.usage_core_nano_seconds?)?;
        let elapsed_ms = u128::try_from(elapsed_ms).ok()?;
        if elapsed_ms == 0 {
            return None;
        }

        // CPU nanoseconds / wall-clock milliseconds * 1000 = nanocores.
        u64::try_from(u128::from(used_ns) * 1_000 / elapsed_ms).ok()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryStats {
    #[serde(rename = "workingSetBytes")]
    pub working_set_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SwapStats {
    #[serde(rename = "swapUsageBytes")]
    pub usage_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Volume {
    #[serde(rename = "availableBytes")]
    pub available_bytes: Option<u64>,
    #[serde(rename = "capacityBytes")]
    pub capacity_bytes: Option<u64>,
    #[serde(rename = "pvcRef")]
    pub pvc_ref: Option<PVCRef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PVCRef {
    pub name: String,
    pub namespace: String,
}

#[cfg(test)]
mod tests {
    use super::StatsSummary;
    use anyhow::Result;
    use serde_json::json;
    use std::assert_matches;

    const STATS_SUMMARY: &str =
        include_str!("../../tests/fixtures/ingress/kubelet_stats_summary.json");

    #[test]
    fn test_parses_kubelet_stats_summary() -> Result<()> {
        let parsed: StatsSummary = serde_json::from_str(STATS_SUMMARY)?;
        assert_eq!(parsed.pods.len(), 14);
        assert_eq!(parsed.node.node_name, "rustik-control-plane");
        assert_eq!(
            parsed.pods[3].pod_ref.name,
            "metrics-cache-5d466b6446-jdjt8"
        );
        assert_eq!(
            parsed.pods[3].pod_ref.uid,
            "b9dd99aa-5cc4-46dc-8f63-1741c27f2b58"
        );
        assert_eq!(parsed.pods[3].containers[0].name, "metrics-cache");
        assert_matches!(
            parsed.pods[3].containers[0]
                .cpu
                .as_ref()
                .and_then(|c| c.usage_nano_cores),
            Some(2232140)
        );
        assert_matches!(
            parsed.pods[3].containers[0]
                .memory
                .as_ref()
                .and_then(|m| m.working_set_bytes),
            Some(10706944)
        );
        assert_matches!(
            parsed.pods[3].containers[0]
                .swap
                .as_ref()
                .and_then(|s| s.usage_bytes),
            Some(0)
        );
        assert_matches!(
            parsed.pods[4].volume.as_ref().map(|m| m[0].available_bytes),
            Some(Some(349491232768))
        );
        assert_matches!(
            parsed.pods[4].volume.as_ref().map(|m| m[0].capacity_bytes),
            Some(Some(1003736440832))
        );
        assert_matches!(
            parsed.pods[4]
                .volume
                .as_ref()
                .and_then(|m| m[0].pvc_ref.as_ref())
                .map(|o| o.name.as_str()),
            Some("pvc-csi-test")
        );
        Ok(())
    }

    fn summary(start_time: &str, cpu_time: &str, usage_core_nano_seconds: u64) -> StatsSummary {
        serde_json::from_value(json!({
            "node": { "nodeName": "node-1" },
            "pods": [{
                "podRef": { "name": "pod-1", "namespace": "default", "uid": "pod-uid" },
                "containers": [{
                    "name": "container-1",
                    "startTime": start_time,
                    "cpu": {
                        "time": cpu_time,
                        "usageNanoCores": 999,
                        "usageCoreNanoSeconds": usage_core_nano_seconds
                    }
                }]
            }]
        }))
        .unwrap()
    }

    #[test]
    fn derives_cpu_rate_and_rejects_restarted_or_reset_containers() {
        let previous = summary(
            "2026-09-04T12:00:00Z",
            "2026-09-04T12:00:00Z",
            1_000_000_000,
        );
        let current = summary(
            "2026-09-04T12:00:00Z",
            "2026-09-04T12:00:10Z",
            3_000_000_000,
        )
        .with_cpu_rates_from(Some(&previous));
        assert_eq!(
            current.pods[0].containers[0]
                .cpu
                .as_ref()
                .unwrap()
                .usage_nano_cores,
            Some(200_000_000)
        );

        let restarted = summary(
            "2026-09-04T12:00:05Z",
            "2026-09-04T12:00:10Z",
            3_000_000_000,
        )
        .with_cpu_rates_from(Some(&previous));
        assert_eq!(
            restarted.pods[0].containers[0]
                .cpu
                .as_ref()
                .unwrap()
                .usage_nano_cores,
            None
        );

        let reset = summary("2026-09-04T12:00:00Z", "2026-09-04T12:00:10Z", 500_000_000)
            .with_cpu_rates_from(Some(&previous));
        assert_eq!(
            reset.pods[0].containers[0]
                .cpu
                .as_ref()
                .unwrap()
                .usage_nano_cores,
            None
        );
    }
}
