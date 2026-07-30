pub mod debug;
mod health;
pub mod ingest;

pub use health::health;

use crate::auth::kubernetes::TokenValidator;
use crate::auth::pull_agent::PullAgentMiddlewareConfig;
use crate::{AppState, auth};
use axum::routing::{get, post};
use axum::{Router, middleware};

pub fn app<V: TokenValidator>(state: AppState<V>, pull: PullAgentMiddlewareConfig) -> Router {
    let ingestion_routes = Router::new()
        .route(
            "/kubelet_stats_summary",
            post(ingest::kubelet_stats_summary),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::kubernetes::authenticate,
        ));

    let pull_agent_routes = Router::new()
        .route("/sections", get(debug::get))
        .route_layer(middleware::from_fn_with_state(
            pull,
            auth::pull_agent::authenticate,
        ));

    Router::new()
        .route("/", get(|| async { "foo" }))
        .nest("/ingest", ingestion_routes)
        .nest("/pull", pull_agent_routes)
        .route("/health", get(health))
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}
