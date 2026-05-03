pub mod metrics_fetcher;

use serde::{Deserialize, Serialize};

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

/// Metadata that does not change once the app has been initialized.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaticMetadata {
    #[serde(rename = "node")]
    pub node_name: String,
    pub host_name: String,
    pub container_platform: Platform,
    pub checkmk_kube_agent: CheckmkKubeAgent,
}
