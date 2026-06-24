use serde::Deserialize;

/// The parsed response from the Kubelet endpoint `/stats/summary`.
#[derive(Clone, Debug, Deserialize)]
pub struct StatsSummary {
    pub node: Node,
    pub pods: Vec<Pod>,
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
}

#[derive(Clone, Debug, Deserialize)]
pub struct Container {
    pub name: String,
    pub cpu: Option<CPUStats>,
    pub memory: Option<MemoryStats>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CPUStats {
    #[serde(rename = "usageNanoCores")]
    pub usage_nano_cores: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryStats {
    #[serde(rename = "workingSetBytes")]
    pub working_set_bytes: Option<u64>,
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
}
