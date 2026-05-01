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

use crate::kube_auth::validate_token_against_kube;

// use cmk_kube_types;

#[derive(Clone)]
struct AppState {
    /// A configured Kubernetes client, ready to validate authentication tokens.
    kube_client: kube::Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli_args::Args::parse();
    let kube_client = kube_auth::kube_client(args.connect_timeout, args.read_timeout).await?;
    let state = AppState { kube_client };
    let app = Router::new()
        .route("/", get(|| async { "foo" }))
        // vvv Routes below this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            authenticate_against_kube,
        ))
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
async fn authenticate_against_kube(
    state: State<AppState>,
    request: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    let token = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let Ok(validation_response) =
        validate_token_against_kube(state.kube_client.clone(), token).await
    else {
        // TODO: Log something (otherwise change this to .unwrap_or(...)?;)
        return Err(StatusCode::NOT_IMPLEMENTED); // Compat with Python cluster-collector
    };

    let Some(status) = validation_response.status else {
        // Should never happen...?
        // TODO: Log something here, too
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    if !status.authenticated.unwrap_or(false) {
        // TODO: And here.
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}
