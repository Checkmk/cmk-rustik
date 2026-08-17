use kube::Client;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use crate::auth::kubernetes::TokenValidator;
use crate::cli_args::CliArgs;
use crate::error::Result;
use crate::host_settings::{AlwaysEmitted, AnnotationKeyPattern, HostSettings};
use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::SystemAgentOutput;
use crate::ingest::kubelet_stats::StatsSummary;
use crate::ingest::reflectors::Stores;

// Used for the size of the various metrics-fetcher caches.
const MAX_SUPPORTED_KUBERNETES_NODES: u64 = 5000;

#[derive(Clone)]
pub struct AppState<V: TokenValidator> {
    pub client: V,
    pub stores: Stores,
    pub reader_allowlist: Vec<String>,
    pub writer_allowlist: Vec<String>,
    pub kubelet_stats_summary_cache: Cache<String, Arc<MetricsFetcherIngestion<StatsSummary>>>,
    pub system_agent_cache: Cache<String, Arc<MetricsFetcherIngestion<SystemAgentOutput>>>,
    pub host_settings: Arc<HostSettings>,
}

impl AppState<Client> {
    pub async fn new(args: &CliArgs, tasks: &mut JoinSet<()>) -> Result<Self> {
        let client = Self::kube_client(args.connect_timeout, args.read_timeout).await?;
        let watcher_client = Self::kube_watcher_client(args.connect_timeout).await?;
        let host_settings = HostSettings {
            cluster_name: args.cluster_name.clone(),
            cluster_host_name: args.cluster_host_name.clone(),
            annotation_key_pattern: AnnotationKeyPattern::new(
                args.import_all_annotations,
                args.annotation_key_pattern.clone(),
            ),
            excluded_node_role_patterns: args.excluded_node_role_patterns.clone(),
            always_emitted: AlwaysEmitted::from_cli_args(args),
        };
        let state = Self {
            client,
            stores: Stores::spawn(watcher_client, tasks),
            reader_allowlist: args.reader_allowlist.clone(),
            writer_allowlist: args.writer_allowlist.clone(),
            kubelet_stats_summary_cache: Cache::builder()
                .max_capacity(MAX_SUPPORTED_KUBERNETES_NODES)
                .build(),
            system_agent_cache: Cache::builder()
                .max_capacity(MAX_SUPPORTED_KUBERNETES_NODES)
                .build(),
            host_settings: host_settings.into(),
        };
        Ok(state)
    }

    /// Build a Kubernetes client for general use (token reviews, etc.).
    async fn kube_client(connect_timeout: Duration, read_timeout: Duration) -> Result<Client> {
        let mut config = kube::Config::infer().await?;
        config.connect_timeout = Some(connect_timeout);
        config.read_timeout = Some(read_timeout);

        Ok(Client::try_from(config)?)
    }

    /// Build a Kubernetes client suitable for watch streams. No read timeout —
    /// watch connections are long-lived and idle between events, so a read timeout
    /// would kill them.
    async fn kube_watcher_client(connect_timeout: Duration) -> Result<Client> {
        let mut config = kube::Config::infer().await?;
        config.connect_timeout = Some(connect_timeout);

        Ok(Client::try_from(config)?)
    }
}

// Intentionally public, provides util functions for other modules
#[cfg(test)]
pub mod tests {
    use k8s_openapi::api::authentication::v1::TokenReview;

    use super::*;

    #[derive(Clone)]
    pub struct MockValidator {
        pub response: std::result::Result<TokenReview, ()>,
    }

    impl TokenValidator for MockValidator {
        type Error = ();
        async fn validate(&self, _token: &str) -> std::result::Result<TokenReview, ()> {
            self.response.clone()
        }
    }

    pub fn test_app_state_with_validator(client: MockValidator) -> AppState<MockValidator> {
        AppState {
            client,
            stores: Default::default(),
            reader_allowlist: vec!["test-ns:test-reader".to_string()],
            writer_allowlist: vec!["test-ns:test-writer".to_string()],
            kubelet_stats_summary_cache: Cache::builder()
                .time_to_live(Duration::from_secs(120))
                .max_capacity(10000)
                .build(),
            system_agent_cache: Cache::builder()
                .time_to_live(Duration::from_secs(120))
                .max_capacity(10000)
                .build(),
            host_settings: HostSettings {
                cluster_name: "testcluster".to_string(),
                cluster_host_name: "testclusterhost".to_string(),
                annotation_key_pattern: AnnotationKeyPattern::ImportAll,
                excluded_node_role_patterns: Vec::new(),
                always_emitted: AlwaysEmitted::default(),
            }
            .into(),
        }
    }

    pub fn test_app_state() -> AppState<MockValidator> {
        test_app_state_with_validator(MockValidator {
            response: Ok(TokenReview::default()),
        })
    }
}
