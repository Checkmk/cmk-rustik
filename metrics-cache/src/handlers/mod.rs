pub mod debug;
mod health;
pub mod ingest;

pub use health::health;

use crate::auth::kubernetes::TokenValidator;
use crate::auth::pull_agent::PullAgentMiddlewareConfig;
use crate::{AppState, auth};
use axum::routing::{get, post};
use axum::{Router, middleware};

pub fn pull_app<V: TokenValidator>(state: AppState<V>, pull: PullAgentMiddlewareConfig) -> Router {
    let routes = Router::new()
        .route("/sections", get(debug::get))
        .route_layer(middleware::from_fn_with_state(
            pull,
            auth::pull_agent::authenticate,
        ));

    Router::new()
        .route("/", get(|| async { "foo" }))
        .nest("/pull", routes)
        .route("/health", get(health))
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}

/// Ingestion shares state with the rest of the application, but is considered
/// its own app for purposes of TLS termination: We want metrics-fetcher to be
/// able to communicate to metrics-cache using a separate set of TLS credentials
/// than anything else which talks to metrics cache (such as an Ingress).
pub fn ingest_app<V: TokenValidator>(state: AppState<V>) -> Router {
    let routes = Router::new()
        .route(
            "/kubelet_stats_summary",
            post(ingest::kubelet_stats_summary),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::kubernetes::authenticate,
        ));

    Router::new()
        .nest("/ingest", routes)
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}
