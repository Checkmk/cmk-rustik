use axum::{Json, extract::State};
use std::sync::Arc;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::ingest::kubelet_stats::StatsSummary;

pub async fn kubelet_stats_summary(
    State(state): State<AppState<impl TokenValidator>>,
    Json(stats_summary): Json<StatsSummary>,
) -> Json<String> {
    state
        .kubelet_stats_summary_cache
        .insert(
            stats_summary.node.node_name.clone(),
            Arc::new(stats_summary),
        )
        .await;
    Json("ok".to_string())
}
