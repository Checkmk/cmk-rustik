use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::ingest::kubelet_health::KubeletHealth;
use crate::ingest::kubelet_stats::StatsSummary;
use crate::ingest::system_agent::SystemAgentOutput;
use crate::ingest::{MetricsFetcherIngestion, MetricsFetcherMetadata};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::header::HeaderMap;
use std::sync::Arc;
use std::time::Instant;

pub async fn kubelet_stats_summary(
    State(state): State<AppState<impl TokenValidator>>,
    headers: HeaderMap,
    Json(stats_summary): Json<StatsSummary>,
) -> Json<String> {
    let node_name = stats_summary.node.node_name.clone();
    let previous = state.kubelet_stats_summary_cache.get(&node_name).await;
    let stats_summary =
        stats_summary.with_cpu_rates_from(previous.as_deref().map(|entry| &entry.payload));
    let ingestion = MetricsFetcherIngestion {
        received_at: Instant::now(),
        metadata: MetricsFetcherMetadata::from(&headers),
        payload: stats_summary,
    };
    state
        .kubelet_stats_summary_cache
        .insert(node_name, Arc::new(ingestion))
        .await;
    Json("ok".to_string())
}

pub async fn kubelet_health(
    State(state): State<AppState<impl TokenValidator>>,
    Path(node_name): Path<String>,
    headers: HeaderMap,
    Json(health): Json<KubeletHealth>,
) -> Json<String> {
    let ingestion = MetricsFetcherIngestion {
        received_at: Instant::now(),
        metadata: MetricsFetcherMetadata::from(&headers),
        payload: health,
    };
    state
        .kubelet_health_cache
        .insert(node_name, Arc::new(ingestion))
        .await;
    Json("ok".to_string())
}

/// Store the raw output of a machine-level agent (currently only Linux's
/// `check_mk_agent`) for a node as-is, keyed by node name. No
/// parsing/validation is done here or by the caller. Kept as [`Bytes`]
/// rather than [`String`] since agent plugins are not guaranteed to produce
/// valid UTF-8.
pub async fn system_agent(
    State(state): State<AppState<impl TokenValidator>>,
    Path(node_name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<String> {
    let ingestion = MetricsFetcherIngestion {
        received_at: Instant::now(),
        metadata: MetricsFetcherMetadata::from(&headers),
        payload: SystemAgentOutput(body),
    };
    state
        .system_agent_cache
        .insert(node_name, Arc::new(ingestion))
        .await;
    Json("ok".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::extract::{Path, State};
    use axum::http::header::HeaderValue;

    use crate::state::tests::test_app_state;
    use crate::test_support::s;

    /// Exercises the handler directly (no router, no auth middleware) — this
    /// is about whether the handler does what's expected of it, not whether
    /// the route is wired up correctly; that's covered in `handlers::tests`.
    #[tokio::test]
    async fn system_agent_populates_cache_and_returns_ok() {
        let state = test_app_state();
        let cache = state.system_agent_cache.clone();
        let mut headers = HeaderMap::new();
        headers.insert("X-Scrape-Time-Ms", HeaderValue::from_static("945"));
        headers.insert("X-Agent-Version", HeaderValue::from_static("3.0.0"));

        let Json(resp) = system_agent(
            State(state),
            Path("node-1".to_string()),
            headers,
            Bytes::from_static(b"<<<check_mk>>>\nVersion: 2.5.0\n"),
        )
        .await;

        assert_eq!(resp, "ok");
        cache.run_pending_tasks().await;
        assert_eq!(
            cache.get("node-1").await.map(|v| v.payload.0.clone()),
            Some(Bytes::from_static(b"<<<check_mk>>>\nVersion: 2.5.0\n"))
        );
        assert_eq!(
            cache
                .get("node-1")
                .await
                .and_then(|v| v.metadata.version.clone()),
            Some(s("3.0.0"))
        );
    }
}
