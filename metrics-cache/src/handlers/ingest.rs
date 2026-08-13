use axum::body::Bytes;
use axum::extract::Path;
use axum::{Json, extract::State};
use std::sync::Arc;
use std::time::Instant;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::SystemAgentOutput;
use crate::ingest::kubelet_stats::StatsSummary;

pub async fn kubelet_stats_summary(
    State(state): State<AppState<impl TokenValidator>>,
    Json(stats_summary): Json<StatsSummary>,
) -> Json<String> {
    let node_name = stats_summary.node.node_name.clone();
    let ingestion = MetricsFetcherIngestion {
        received_at: Instant::now(),
        payload: stats_summary,
    };
    state
        .kubelet_stats_summary_cache
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
    body: Bytes,
) -> Json<String> {
    let ingestion = MetricsFetcherIngestion {
        received_at: Instant::now(),
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
    use axum::extract::{Path, State};

    use super::*;
    use crate::state::tests::test_app_state;

    /// Exercises the handler directly (no router, no auth middleware) — this
    /// is about whether the handler does what's expected of it, not whether
    /// the route is wired up correctly; that's covered in `handlers::tests`.
    #[tokio::test]
    async fn system_agent_populates_cache_and_returns_ok() {
        let state = test_app_state();
        let cache = state.system_agent_cache.clone();

        let Json(resp) = system_agent(
            State(state),
            Path("node-1".to_string()),
            Bytes::from_static(b"<<<check_mk>>>\nVersion: 2.5.0\n"),
        )
        .await;

        assert_eq!(resp, "ok");
        cache.run_pending_tasks().await;
        assert_eq!(
            cache.get("node-1").await.map(|v| v.payload.0.clone()),
            Some(Bytes::from_static(b"<<<check_mk>>>\nVersion: 2.5.0\n"))
        );
    }
}
