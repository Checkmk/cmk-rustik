use serde::{Deserialize, Serialize};

use cmk_kube_types::metadata::{CheckmkKubeAgent, Platform};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheSizeInfo {
    size: u32,
    maxsize: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheHealth {
    container_metrics: CacheSizeInfo,
    machine_sections: CacheSizeInfo,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MetricsCacheMetadata {
    pub node: String,
    pub host_name: String,
    pub container_platform: Platform,
    pub checkmk_kube_agent: CheckmkKubeAgent,
    pub cache_health: CacheHealth,
}
