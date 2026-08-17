pub(crate) mod health;
pub(crate) mod ingest;
pub(crate) mod pull;

use crate::auth::kubernetes::TokenValidator;
use crate::auth::pull_agent::PullAgentMiddlewareConfig;
use crate::{AppState, auth};
use axum::routing::{get, post};
use axum::{Router, middleware};

pub fn pull_app<V: TokenValidator>(state: AppState<V>, pull: PullAgentMiddlewareConfig) -> Router {
    let routes = Router::new()
        .route("/sections", get(pull::get))
        .route_layer(middleware::from_fn_with_state(
            pull,
            auth::pull_agent::authenticate,
        ));

    Router::new()
        .route("/", get(|| async { "foo" }))
        .nest("/pull", routes)
        .route("/health", get(health::health))
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
        .route("/system_agent/{node_name}", post(ingest::system_agent))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::kubernetes::authenticate,
        ));

    Router::new()
        .nest("/ingest", routes)
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::state::tests::test_app_state;

    /// Sanity check that the ingest route is actually wired up behind the
    /// kubernetes-token middleware; handler-level behavior is covered in
    /// `handlers::ingest::tests`.
    #[tokio::test]
    async fn system_agent_ingest_requires_auth() {
        let state = test_app_state();
        let app = ingest_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ingest/system_agent/node-1")
                    .body(Body::from("<<<check_mk>>>\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
