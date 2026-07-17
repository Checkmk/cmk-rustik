use axum::{Json, extract::State};
use std::sync::Arc;
use std::time::Instant;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::ingest::MetricsFetcherIngestion;
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
