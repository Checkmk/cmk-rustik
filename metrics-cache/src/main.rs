mod auth;
mod cli_args;
mod handlers;
mod kube_auth;

use anyhow::Result;
use axum::{
    Router, middleware,
    routing::{get, post},
};
use clap::Parser;
use moka::future::Cache;
use std::sync::Arc;

use crate::auth::authenticate;
use crate::kube_auth::TokenValidator;
use cmk_kube_types::{machine_sections, metadata};

#[derive(Clone)]
pub struct AppState<V: TokenValidator> {
    pub validator: V,
    pub reader_allowlist: Vec<String>,
    pub writer_allowlist: Vec<String>,
    pub metrics_cache_static_metadata: Arc<metadata::StaticMetadata>,
    pub machine_sections_cache: Cache<String, machine_sections::FetchResult>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli_args::Args::parse();
    let validator = kube_auth::kube_client(args.connect_timeout, args.read_timeout).await?;
    let static_metadata = handlers::metadata::generate_static_metadata()?;
    let state = AppState {
        validator,
        reader_allowlist: args.reader_allowlist,
        writer_allowlist: args.writer_allowlist,
        metrics_cache_static_metadata: Arc::new(static_metadata),
        machine_sections_cache: Cache::builder()
            .time_to_live(args.cache_ttl)
            .max_capacity(args.cache_maxsize)
            .build(),
    };
    let app = Router::new()
        .route("/", get(|| async { "foo" }))
        .route("/metadata", get(handlers::metadata::get))
        .route("/machine_sections", get(handlers::machine_sections::get))
        .route(
            "/update_machine_sections",
            post(handlers::machine_sections::update),
        )
        // vvv Routes above this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        // ^^^ Routes below this will be PUBLIC ^^^
        .route("/health", get(handlers::health))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind((args.address.as_str(), args.port))
        .await
        .unwrap();
    axum::serve(listener, app).await?;
    Ok(())
}
