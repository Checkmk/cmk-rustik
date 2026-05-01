mod cli_args;
mod kube_auth;

use anyhow::Result;
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use clap::Parser;

use crate::kube_auth::TokenValidator;

#[derive(Clone)]
struct AppState<V: TokenValidator> {
    validator: V,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli_args::Args::parse();
    let validator = kube_auth::kube_client(args.connect_timeout, args.read_timeout).await?;
    let state = AppState { validator };
    let app = Router::new()
        .route("/", get(|| async { "foo" }))
        // vvv Routes below this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        // ^^^ Routes below this will be PUBLIC ^^^
        .route("/health", get(|| async { "Stayin' alive" }))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind((args.address.as_str(), args.port))
        .await
        .unwrap();
    axum::serve(listener, app).await?;
    Ok(())
}

/// Middleware function that handles authentication.
///
/// Every endpoint affected by this middleware will trigger an authentication
/// request to Kubernetes and must be successful. Otherwise, the request is
/// aborted.
async fn authenticate<V: TokenValidator>(
    state: State<AppState<V>>,
    request: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    let token = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let Ok(validation_response) = state.validator.validate(token).await else {
        // TODO: Log something (otherwise change this to .unwrap_or(...)?;)
        return Err(StatusCode::NOT_IMPLEMENTED); // Compat with Python cluster-collector
    };

    let Some(status) = validation_response.status else {
        // Should never happen...?
        // TODO: Log something here, too
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    if !status.authenticated.unwrap_or(false) {
        // TODO: And here.
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewStatus};
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
        let state = AppState { validator };
        Router::new()
            .route("/", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
            .with_state(state)
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
        for case in [Some(false), None] {
            let app = app_with_mock(MockValidator {
                response: Ok(TokenReview {
                    status: Some(TokenReviewStatus {
                        authenticated: case,
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

    // Valid authenticated token should pass through to the handler.
    #[tokio::test]
    async fn authenticated_passes_through() {
        let app = app_with_mock(MockValidator {
            response: Ok(TokenReview {
                status: Some(TokenReviewStatus {
                    authenticated: Some(true),
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
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
