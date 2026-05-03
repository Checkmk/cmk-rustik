use serde::{Deserialize, Serialize};

use crate::metadata::StaticMetadata;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FetcherKind {
    #[serde(rename = "Container Metrics")]
    Container,
    #[serde(rename = "Machine Sections")]
    Machine,
}

/// Which metrics-fetcher did the data come from?
///
/// Ideally an enum, but for now maintaining compatibility with Python takes
/// takes precedence.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SourceVersion {
    pub cadvisor_version: Option<String>,
    pub checkmk_agent_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Metadata {
    #[serde(flatten)]
    pub static_metadata: StaticMetadata,
    pub collector_type: FetcherKind,
    pub components: SourceVersion,
}
