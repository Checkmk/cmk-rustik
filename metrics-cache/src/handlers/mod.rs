pub mod debug;
mod health;
pub mod ingest;
pub mod machine_sections;
pub mod metadata;

pub use health::health;

use crate::auth::kubernetes::TokenValidator;
use crate::{AppState, auth};
use axum::routing::{get, post};
use axum::{Router, middleware};

pub fn app<V: TokenValidator>(state: AppState<V>) -> Router {
    Router::new()
        .route("/", get(|| async { "foo" }))
        .route("/metadata", get(metadata::get))
        .route("/machine_sections", get(machine_sections::get))
        .route("/update_machine_sections", post(machine_sections::update))
        .route(
            "/ingest/kubelet_stats_summary",
            post(ingest::kubelet_stats_summary),
        )
        // vvv Routes above this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::authenticate,
        ))
        // ^^^ Routes below this will be PUBLIC ^^^
        .route("/health", get(health))
        .route("/debug", get(debug::get))
        .with_state(state)
}
