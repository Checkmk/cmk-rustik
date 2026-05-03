use axum::{Json, extract::State};

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use cmk_kube_types::container_metrics::{ContainerMetrics, Metric};

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Json<Vec<Metric>> {
    Json(
        state
            .container_metrics_cache
            .iter()
            .map(|(_, v)| v)
            .collect(),
    )
}

pub async fn update(
    State(state): State<AppState<impl TokenValidator>>,
    Json(container_metrics): Json<ContainerMetrics>,
) -> Json<String> {
    // Add all of them to the cache
    for metric in container_metrics.container_metrics {
        let key = format!("{}:{}", metric.container_name, metric.metric_name,);
        state.container_metrics_cache.insert(key, metric).await;
    }

    // And its metadata
    let metadata_key = format!(
        "container_metrics:{}",
        container_metrics.metadata.static_metadata.node
    );
    state
        .metrics_fetcher_metadata_cache
        .insert(metadata_key, container_metrics.metadata)
        .await;
    Json("ok".to_string())
}
