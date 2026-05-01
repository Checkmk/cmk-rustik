mod auth;
mod cli_args;
mod kube_auth;

use anyhow::Result;
use axum::{Router, middleware, routing::get};
use clap::Parser;

use crate::auth::authenticate;
use crate::kube_auth::TokenValidator;

#[derive(Clone)]
pub struct AppState<V: TokenValidator> {
    pub validator: V,
    pub reader_allowlist: Vec<String>,
    pub writer_allowlist: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli_args::Args::parse();
    let validator = kube_auth::kube_client(args.connect_timeout, args.read_timeout).await?;
    let state = AppState {
        validator,
        reader_allowlist: args.reader_allowlist,
        writer_allowlist: args.writer_allowlist,
    };
    let app = Router::new()
        .route("/", get(|| async { "foo" }))
        // vvv Routes below this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        // ^^^ Routes above this will be PUBLIC ^^^
        .route("/health", get(|| async { "Stayin' alive" }))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind((args.address.as_str(), args.port))
        .await
        .unwrap();
    axum::serve(listener, app).await?;
    Ok(())
}
