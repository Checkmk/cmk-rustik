use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};
use k8s_openapi::api::authentication::v1::TokenReviewStatus;

use crate::AppState;
use crate::kube_auth::TokenValidator;

/// Extract bearer token from Authorization header.
fn extract_bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

/// Extract the service account name from a TokenReviewStatus.
/// Returns the part after "system:serviceaccount:" (e.g., "namespace:name").
fn extract_service_account(status: &TokenReviewStatus) -> Option<&str> {
    status
        .user
        .as_ref()?
        .username
        .as_ref()?
        .strip_prefix("system:serviceaccount:")
}

/// Check if the service account is allowed for the given HTTP method.
/// Write access also implies read access.
fn is_allowed<V: TokenValidator>(account: &str, method: &Method, state: &AppState<V>) -> bool {
    match *method {
        Method::GET | Method::HEAD => {
            state.reader_allowlist.contains(&account.to_string())
                || state.writer_allowlist.contains(&account.to_string())
        }
        Method::POST | Method::PUT | Method::DELETE | Method::PATCH => {
            state.writer_allowlist.contains(&account.to_string())
        }
        _ => false,
    }
}

/// Middleware that authenticates requests against Kubernetes.
///
/// 1. Extracts bearer token from Authorization header
/// 2. Validates token with Kubernetes TokenReview API
/// 3. Checks service account against reader/writer allowlists based on HTTP method
pub async fn authenticate<V: TokenValidator>(
    state: State<AppState<V>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer_token(&request).ok_or(StatusCode::UNAUTHORIZED)?;

    let review = state
        .validator
        .validate(token)
        .await
        .map_err(|_| StatusCode::NOT_IMPLEMENTED)?; // Compat with Python cluster-collector

    let status = review.status.ok_or(StatusCode::NOT_IMPLEMENTED)?;

    if !status.authenticated.unwrap_or(false) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let account = extract_service_account(&status).ok_or(StatusCode::NOT_IMPLEMENTED)?;

    if !is_allowed(account, request.method(), &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use axum::{Router, body::Body, http::Request, middleware, routing::get};
    use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewStatus, UserInfo};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockValidator {
        response: std::result::Result<TokenReview, ()>,
    }

    impl TokenValidator for MockValidator {
        async fn validate(&self, _token: &str) -> Result<TokenReview> {
            self.response
                .clone()
                .map_err(|_| anyhow::anyhow!("mock error"))
        }
    }

    fn app_with_mock(validator: MockValidator) -> Router {
        let state = AppState {
            validator,
            reader_allowlist: vec!["test-ns:test-reader".to_string()],
            writer_allowlist: vec!["test-ns:test-writer".to_string()],
        };
        Router::new()
            .route("/", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
            .with_state(state)
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

    // Request without Authorization header should be rejected.
    #[tokio::test]
    async fn missing_auth_header_returns_unauthorized() {
        let app = app_with_mock(MockValidator {
            response: Ok(TokenReview::default()),
        });
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // Kubernetes API errors should return 501 (compat with Python cluster-collector).
    #[tokio::test]
    async fn validator_error_returns_not_implemented() {
        let app = app_with_mock(MockValidator { response: Err(()) });
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    // TokenReview response without status field should return 501.
    #[tokio::test]
    async fn missing_status_returns_not_implemented() {
        let app = app_with_mock(MockValidator {
            response: Ok(TokenReview::default()),
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    // Token that Kubernetes says is not authenticated should be rejected.
    #[tokio::test]
    async fn unauthenticated_returns_unauthorized() {
        for authenticated in [Some(false), None] {
            let app = app_with_mock(MockValidator {
                response: Ok(TokenReview {
                    status: Some(TokenReviewStatus {
                        authenticated,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            });
            let resp = app
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .header("Authorization", "Bearer test-token")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        }
    }

    // Username without service account prefix should be rejected.
    #[tokio::test]
    async fn non_service_account_returns_not_implemented() {
        let app = app_with_mock(MockValidator {
            response: Ok(authenticated_review("regular-user")),
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    // Service account not in allowlist should be rejected.
    #[tokio::test]
    async fn service_account_not_in_allowlist_returns_unauthorized() {
        let app = app_with_mock(MockValidator {
            response: Ok(authenticated_review("system:serviceaccount:other-ns:other")),
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // Allowed reader service account should pass through on GET.
    #[tokio::test]
    async fn allowed_reader_passes_through() {
        let app = app_with_mock(MockValidator {
            response: Ok(authenticated_review(
                "system:serviceaccount:test-ns:test-reader",
            )),
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
