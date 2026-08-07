use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use std::io;
use std::net::SocketAddr;
use tokio::task::JoinSet;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use metrics_cache::auth::pull_agent::PullAgentMiddlewareConfig;
use metrics_cache::cli_args::CliArgs;
use metrics_cache::handlers;
use metrics_cache::otel::client::OtelClient;
use metrics_cache::otel::otel_loop;
use metrics_cache::push::client::CheckmkPushClient;
use metrics_cache::push::push_loop;
use metrics_cache::push::register::CheckmkPushRegistration;
use metrics_cache::startup::tls;
use metrics_cache::state::AppState;

fn init_tracing(args: &CliArgs) {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level)),
        )
        .with_target(false)
        .with_writer(io::stderr)
        .init();
}

async fn bind(
    address: &str,
    port: u16,
    tls: Option<RustlsConfig>,
    app: axum::Router,
) -> anyhow::Result<()> {
    let addr = SocketAddr::new(address.parse()?, port);
    match tls {
        None => {
            info!("metrics-cache binding to {} (non-TLS)", addr);
            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await?;
        }
        Some(config) => {
            info!("metrics-cache binding to {} (TLS)", addr);
            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service())
                .await?;
        }
    }

    Ok(())
}

fn pull_agent_middleware(args: &CliArgs) -> anyhow::Result<PullAgentMiddlewareConfig> {
    match (
        args.disable_pull_authentication,
        args.pull_shared_secret.as_deref(),
    ) {
        // A shared secret is configured but authentication is disabled. Which
        // is meant?
        (true, Some(_)) => anyhow::bail!(
            "--disable-pull-authentication is set, but a pull shared secret is also \
             configured. These contradict each other; set exactly one of them."
        ),
        // Deliberately public pull-mode endpoints.
        (true, None) => {}
        // Auth is desired, but secret is empty, default closed.
        (false, Some("")) => warn!(
            "The pull shared secret is set but empty; all pull requests will be \
             rejected. Check the Kubernetes secret (and key) the chart references."
        ),
        // Normal: secret configured or pull mode unconfigured.
        (false, _) => {}
    }
    Ok(PullAgentMiddlewareConfig {
        auth_enabled: !args.disable_pull_authentication,
        shared_secret: args.pull_shared_secret.clone(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    init_tracing(&args);

    // We use ring instead of aws-lc-rs
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let mut reflector_tasks = JoinSet::new();
    let state = AppState::new(&args, &mut reflector_tasks).await?;
    let pull_app = handlers::pull_app(state.clone(), pull_agent_middleware(&args)?);
    let ingest_app = handlers::ingest_app(state.clone());

    let push_client = match &args.push_receiver {
        Some(base_url) => {
            info!("Push receiver enabled, will push sections to Checkmk server");
            let registration = CheckmkPushRegistration::new(state.clone().client, &args);
            let secret = registration.register_if_needed().await?;
            Some(CheckmkPushClient::from_secret(base_url, &secret)?)
        }
        None => None,
    };

    let otel_client = match &args.otel_endpoint {
        Some(base_url) => {
            info!("OpenTelemetry enabled, will push metrics to OpenTelemetry collector");
            // TODO: Auth, etc.
            Some(OtelClient::new(base_url))
        }
        None => None,
    };

    let pull_tls_config = tls::resolve(state.clone().client, args.pull_tls_config()).await?;
    let ingest_tls_config = tls::resolve(state.clone().client, args.ingest_tls_config()).await?;

    tokio::select! {
        // Pull mode listener
        res = bind(
            &args.pull_address,
            args.pull_port,
            pull_tls_config,
            pull_app,
        ) => {
            if let Err(e) = res {
                tracing::error!(error = %e, "Axum pull-mode server exited with error");
            }
            anyhow::bail!("Axum pull-mode server terminated unexpectedly");
        }

        // Intra-cluster ingest mode listener
        res = bind(
            &args.ingest_address,
            args.ingest_port,
            ingest_tls_config,
            ingest_app,
        ) => {
            if let Err(e) = res {
                tracing::error!(error = %e, "Axum intra-cluster ingest server exited with error");
            }
            anyhow::bail!("Axum intra-cluster ingest server terminated unexpectedly");
        }

        // Reflector tasks
        res = reflector_tasks.join_next() => {
            tracing::error!(error = ?res, "Reflector task exited unexpectedly");
            anyhow::bail!("Reflector task terminated unexpectedly");
        }

        // Push to Checkmk server
        res = async {
            match push_client {
                Some(client) => push_loop(client, state.clone(), args.push_interval).await,
                None => std::future::pending().await,
            }
        } => {
            if let Err(e) = res {
                tracing::error!(error = %e, "push loop exited with error");
            }
            anyhow::bail!("push loop terminated unexpectedly");
        }

        // Push to OpenTelemetry collector
        () = async {
            match otel_client {
                Some(client) => otel_loop(client, state.clone()).await,
                None => std::future::pending().await,
            }
        } => {
            tracing::error!("OpenTelemetry loop exited unexpectedly");
            anyhow::bail!("OpenTelemetry loop terminated unexpectedly");
        }
    }
}
