use axum::extract::Path;
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

/// Store the raw check_mk_agent output for a node, verbatim, keyed by node
/// name. No parsing/validation is done here or by the caller.
pub async fn linux_agent(
    State(state): State<AppState<impl TokenValidator>>,
    Path(node_name): Path<String>,
    body: String,
) -> Json<String> {
    state
        .linux_agent_cache
        .insert(node_name, Arc::new(body))
        .await;
    Json("ok".to_string())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewStatus, UserInfo};
    use tower::ServiceExt;

    use crate::auth::pull_agent::PullAgentMiddlewareConfig;
    use crate::handlers::app;
    use crate::state::tests::{MockValidator, test_app_state_with_validator};

    fn no_pull_agent() -> PullAgentMiddlewareConfig {
        PullAgentMiddlewareConfig {
            auth_enabled: false,
            shared_secret: None,
        }
    }

    fn authenticated_review(username: &str) -> TokenReview {
        TokenReview {
            status: Some(TokenReviewStatus {
                authenticated: Some(true),
                user: Some(UserInfo {
                    username: Some(username.to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn linux_agent_ingest_populates_cache_and_returns_ok() {
        let state = test_app_state_with_validator(MockValidator {
            response: Ok(authenticated_review(
                "system:serviceaccount:test-ns:test-writer",
            )),
        });
        let cache = state.linux_agent_cache.clone();
        let app = app(state, no_pull_agent());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ingest/linux_agent/node-1")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::from("<<<check_mk>>>\nVersion: 2.5.0\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        cache.run_pending_tasks().await;
        assert_eq!(
            cache.get("node-1").await.map(|v| (*v).clone()),
            Some("<<<check_mk>>>\nVersion: 2.5.0\n".to_string())
        );
    }

    #[tokio::test]
    async fn linux_agent_ingest_requires_auth() {
        let state = test_app_state_with_validator(MockValidator {
            response: Ok(TokenReview::default()),
        });
        let app = app(state, no_pull_agent());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ingest/linux_agent/node-1")
                    .body(Body::from("<<<check_mk>>>\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
