pub mod debug;
mod health;
pub mod ingest;

pub use health::health;

use crate::auth::kubernetes::TokenValidator;
use crate::{AppState, auth};
use axum::routing::{get, post};
use axum::{Router, middleware};

pub fn app<V: TokenValidator>(state: AppState<V>) -> Router {
    Router::new()
        .route("/", get(|| async { "foo" }))
        .route(
            "/ingest/kubelet_stats_summary",
            post(ingest::kubelet_stats_summary),
        )
        // vvv Routes above this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::kubernetes::authenticate,
        ))
        // ^^^ Routes below this will be PUBLIC ^^^
        .route("/health", get(health))
        .route("/debug", get(debug::get))
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}
