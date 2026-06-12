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
