use serde::{Deserialize, Serialize};

use crate::metadata::Platform;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FetcherKind {
    #[serde(rename = "Container Metrics")]
    Container,
    #[serde(rename = "Machine Sections")]
    Machine,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SourceVersion {
    pub cadvisor_version: Option<String>,
    pub checkmk_agent_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckmkKubeAgent {
    pub project_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fetcher {
    pub node: String,
    pub host_name: String,
    pub container_platform: Platform,
    pub checkmk_kube_agent: CheckmkKubeAgent,
    pub collector_type: FetcherKind,
    pub components: SourceVersion,
}
