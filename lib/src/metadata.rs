use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Fetcher {
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
pub struct Platform {
    pub os_name: String,
    pub os_version: String,
    pub python_version: String, // Not used in Checkmk, but still a required field
    pub python_compiler: String, // Not used in Checkmk, but still a required field
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckmkKubeAgent {
    pub project_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MetricsFetcher {
    pub node: String,
    pub host_name: String,
    pub container_platform: Platform,
    pub checkmk_kube_agent: CheckmkKubeAgent,
    pub collector_type: Fetcher,
    pub components: SourceVersion,
}
