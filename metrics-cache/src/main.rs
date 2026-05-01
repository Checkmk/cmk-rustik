use axum::{Router, routing::get};

// use cmk_kube_types;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "foo" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
