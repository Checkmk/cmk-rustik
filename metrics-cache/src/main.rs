use axum::{
    Router, middleware,
    routing::{get, post},
};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use moka::future::Cache;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use metrics_cache::{AppState, Stores, auth, cli_args, handlers, ingest::reflectors};

// Kubernetes can have a maximum of 5000 nodes, and we currently run two
// metrics-fetchers per node (container_metrics and machine_sections).
const METRICS_FETCHER_METADATA_CACHE_MAX_SIZE: u64 = 10000;

// Used for the size of the various metrics-fetcher caches.
const MAX_SUPPORTED_KUBERNETES_NODES: u64 = 5000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli_args::Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level)),
        )
        .with_target(false)
        .init();
    let client = metrics_cache::kube::client(args.connect_timeout, args.read_timeout).await?;
    let watcher_client = metrics_cache::kube::watcher_client(args.connect_timeout).await?;
    let static_metadata = handlers::metadata::generate_static_metadata()?;

    let pod_store = reflectors::pods(watcher_client.clone());
    let node_store = reflectors::nodes(watcher_client.clone());
    let deployment_store = reflectors::deployments(watcher_client.clone());
    let daemonset_store = reflectors::daemonsets(watcher_client.clone());
    let namespace_store = reflectors::namespaces(watcher_client.clone());
    let replicaset_store = reflectors::replicasets(watcher_client);
    let stores = Stores {
        pods: pod_store,
        nodes: node_store,
        deployments: deployment_store,
        daemonsets: daemonset_store,
        namespaces: namespace_store,
        replicasets: replicaset_store,
    };

    let state = AppState {
        client,
        stores,
        reader_allowlist: args.reader_allowlist,
        writer_allowlist: args.writer_allowlist,
        metrics_cache_static_metadata: Arc::new(static_metadata),
        machine_sections_cache: Cache::builder()
            .time_to_live(args.cache_ttl)
            .max_capacity(args.cache_maxsize)
            .build(),
        metrics_fetcher_metadata_cache: Cache::builder()
            .time_to_live(args.cache_ttl)
            .max_capacity(METRICS_FETCHER_METADATA_CACHE_MAX_SIZE)
            .build(),
        kubelet_stats_summary_cache: Cache::builder()
            .max_capacity(MAX_SUPPORTED_KUBERNETES_NODES)
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
        .route(
            "/ingress/kubelet_stats_summary",
            post(handlers::ingress::kubelet_stats_summary),
        )
        // vvv Routes above this will REQUIRE AUTHENTICATION vvv
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::authenticate,
        ))
        // ^^^ Routes below this will be PUBLIC ^^^
        .route("/health", get(handlers::health))
        .route("/debug", get(handlers::debug::get))
        .with_state(state);

    let addr = SocketAddr::new(args.address.parse()?, args.port);
    if args.secure_protocol {
        let (Some(keyfile), Some(certfile)) = (args.ssl_keyfile, args.ssl_certfile) else {
            anyhow::bail!(
                "Both --keyfile and --certfile must be provided when --secure-protocol is enabled"
            );
        };
        debug!("Reading key {} and cert {}", keyfile, certfile);
        let config = RustlsConfig::from_pem_file(keyfile, certfile).await?;
        info!("metrics-cache binding to {} (TLS)", addr);
        axum_server::bind_rustls(addr, config)
            .serve(app.into_make_service())
            .await?;
    } else {
        info!("metrics-cache binding to {}", addr);
        axum_server::bind(addr)
            .serve(app.into_make_service())
            .await?;
    }

    Ok(())
}
