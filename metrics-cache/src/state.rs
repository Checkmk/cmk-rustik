use kube::Client;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

use crate::auth::kubernetes::TokenValidator;
use crate::cli_args::CliArgs;
use crate::error::Result;
use crate::ingest::kubelet_stats::StatsSummary;
use crate::ingest::reflectors::Stores;
use cmk_kube_types::{machine_sections, metadata};

// Kubernetes can have a maximum of 5000 nodes, and we currently run two
// metrics-fetchers per node (container_metrics and machine_sections).
const METRICS_FETCHER_METADATA_CACHE_MAX_SIZE: u64 = 10000;

// Used for the size of the various metrics-fetcher caches.
const MAX_SUPPORTED_KUBERNETES_NODES: u64 = 5000;

#[derive(Clone)]
pub struct AppState<V: TokenValidator> {
    pub client: V,
    pub stores: Stores,
    pub reader_allowlist: Vec<String>,
    pub writer_allowlist: Vec<String>,
    pub metrics_cache_static_metadata: Arc<metadata::StaticMetadata>,
    pub machine_sections_cache: Cache<String, machine_sections::Sections>,
    pub metrics_fetcher_metadata_cache: Cache<String, metadata::metrics_fetcher::Metadata>,
    pub kubelet_stats_summary_cache: Cache<String, Arc<StatsSummary>>,
}

impl AppState<Client> {
    pub async fn new(args: &CliArgs) -> Result<Self> {
        let client = Self::kube_client(args.connect_timeout, args.read_timeout).await?;
        let watcher_client = Self::kube_watcher_client(args.connect_timeout).await?;
        let static_metadata = crate::handlers::metadata::generate_static_metadata()?;
        let state = Self {
            client,
            stores: Stores::spawn(watcher_client),
            reader_allowlist: args.reader_allowlist.clone(),
            writer_allowlist: args.writer_allowlist.clone(),
            metrics_cache_static_metadata: Arc::new(static_metadata),
            machine_sections_cache: Cache::builder()
                .time_to_live(args.cache_ttl)
                .max_capacity(args.cache_maxsize)
                .build(),
            metrics_fetcher_metadata_cache: Cache::builder()
                .time_to_live(args.cache_ttl)
                .max_capacity(METRICS_FETCHER_METADATA_CACHE_MAX_SIZE)
                .build(),
            kubelet_stats_summary_cache: Cache::builder()
                .max_capacity(MAX_SUPPORTED_KUBERNETES_NODES)
                .build(),
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
    use cmk_kube_types::metadata;

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
        let (pod_store, _) = kube::runtime::reflector::store();
        let (node_store, _) = kube::runtime::reflector::store();
        let (deployment_store, _) = kube::runtime::reflector::store();
        let (daemonset_store, _) = kube::runtime::reflector::store();
        let (namespace_store, _) = kube::runtime::reflector::store();
        let (replicaset_store, _) = kube::runtime::reflector::store();
        AppState {
            client,
            stores: Stores {
                pods: pod_store,
                nodes: node_store,
                deployments: deployment_store,
                daemonsets: daemonset_store,
                namespaces: namespace_store,
                replicasets: replicaset_store,
            },
            reader_allowlist: vec!["test-ns:test-reader".to_string()],
            writer_allowlist: vec!["test-ns:test-writer".to_string()],
            metrics_cache_static_metadata: Arc::new(metadata::StaticMetadata {
                node_name: "test-node".to_string(),
                host_name: "test-host".to_string(),
                container_platform: metadata::Platform {
                    os_name: "linux".to_string(),
                    os_version: "5.15".to_string(),
                    python_version: "3.9".to_string(),
                    python_compiler: "GCC 10.2".to_string(),
                },
                checkmk_kube_agent: metadata::CheckmkKubeAgent {
                    project_version: "1.0.0".to_string(),
                },
            }),
            machine_sections_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(120))
                .build(),
            metrics_fetcher_metadata_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(120))
                .max_capacity(10000)
                .build(),
            kubelet_stats_summary_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(120))
                .max_capacity(10000)
                .build(),
        }
    }

    pub fn test_app_state() -> AppState<MockValidator> {
        test_app_state_with_validator(MockValidator {
            response: Ok(TokenReview::default()),
        })
    }
}
