use anyhow::Result;
use axum::{Json, extract::State};
use clap::crate_version;
use moka::future::Cache;
use os_release::OsRelease;
use serde::{Deserialize, Serialize};
use std::env;

use crate::AppState;
use crate::kube_auth::TokenValidator;
use cmk_kube_types::metadata::{self, CheckmkKubeAgent, Platform, StaticMetadata};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheSizeInfo {
    size: u64,
    maxsize: Option<u64>, // We require it, but moka does not
}

impl CacheSizeInfo {
    async fn from_cache<K, V>(cache: Cache<K, V>) -> Self
    where
        K: std::hash::Hash + Eq + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        cache.run_pending_tasks().await;
        CacheSizeInfo {
            size: cache.entry_count(),
            maxsize: cache.policy().max_capacity(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheHealth {
    container_metrics: CacheSizeInfo,
    machine_sections: CacheSizeInfo,
}

impl CacheHealth {
    async fn from_state(state: AppState<impl TokenValidator>) -> Self {
        CacheHealth {
            container_metrics: CacheSizeInfo {
                // TODO
                size: 0,
                maxsize: Some(0),
            },
            machine_sections: CacheSizeInfo::from_cache(state.machine_sections_cache).await,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MetricsCacheMetadata {
    #[serde(flatten)]
    static_metadata: StaticMetadata,
    cache_health: CacheHealth,
}

impl MetricsCacheMetadata {
    async fn from_state(state: AppState<impl TokenValidator>) -> Self {
        MetricsCacheMetadata {
            static_metadata: (*state.metrics_cache_static_metadata).clone(),
            cache_health: CacheHealth::from_state(state).await,
        }
    }
}

/// The response sent from the [`get()`] handler for the metadata endpoint.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MetadataResponse {
    cluster_collector_metadata: MetricsCacheMetadata,
    node_collector_metadata: Vec<metadata::metrics_fetcher::Metadata>,
}

impl MetadataResponse {
    async fn from_state(state: AppState<impl TokenValidator>) -> Self {
        MetadataResponse {
            cluster_collector_metadata: MetricsCacheMetadata::from_state(state).await,
            node_collector_metadata: vec![], // TODO
        }
    }
}

fn get_env_var(var_name: &str) -> Result<String> {
    env::var(var_name).map_err(|e| anyhow::anyhow!("Failed to get {}: {}", var_name, e))
}

/// Generate metadata for this instance of metrics-cache. Intended to be called
/// once at startup and stored in Axum State.
pub fn generate_static_metadata() -> Result<StaticMetadata> {
    let node = get_env_var("NODE_NAME")?;
    let host_name = get_env_var("HOSTNAME")?;
    let os_release = OsRelease::new()?;
    Ok(StaticMetadata {
        node,
        host_name,
        container_platform: Platform {
            os_name: os_release.id,
            os_version: os_release.version_id,
            python_version: String::new(),
            python_compiler: String::new(),
        },
        checkmk_kube_agent: CheckmkKubeAgent {
            project_version: crate_version!().to_string(),
        },
    })
}

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Json<MetadataResponse> {
    Json(MetadataResponse::from_state(state).await)
}
