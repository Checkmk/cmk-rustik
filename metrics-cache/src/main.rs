mod cli_args;

use axum::{Router, routing::get};
use clap::Parser;

// use cmk_kube_types;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "foo" }));

    let args = cli_args::Args::parse();
    let listener = tokio::net::TcpListener::bind((args.address, args.port))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
