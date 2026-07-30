use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use std::io;
use std::net::SocketAddr;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

use metrics_cache::auth::pull_agent::PullAgentMiddlewareConfig;
use metrics_cache::cli_args::CliArgs;
use metrics_cache::handlers;
use metrics_cache::otel::client::OtelClient;
use metrics_cache::otel::otel_loop;
use metrics_cache::push::client::CheckmkPushClient;
use metrics_cache::push::push_loop;
use metrics_cache::push::register::CheckmkPushRegistration;
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

async fn bind(args: &CliArgs, app: axum::Router) -> anyhow::Result<()> {
    let addr = SocketAddr::new(args.address.parse()?, args.port);
    if args.secure_protocol {
        let (Some(keyfile), Some(certfile)) = (&args.ssl_keyfile, &args.ssl_certfile) else {
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
    let mut reflector_tasks = JoinSet::new();
    let state = AppState::new(&args, &mut reflector_tasks).await?;
    let app = handlers::app(state.clone(), pull_agent_middleware(&args)?);

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

    tokio::select! {
        // HTTP (pull mode, metrics-fetcher ingestion)
        res = bind(&args, app) => {
            if let Err(e) = res {
                tracing::error!(error = %e, "Axum server exited with error");
            }
            anyhow::bail!("Axum server terminated unexpectedly");
        }

        // Reflector tasks
        res = reflector_tasks.join_next() => {
            tracing::error!(error = ?res, "Reflector task exited unexpectedly");
            anyhow::bail!("Reflector task terminated unexpectedly");
        }

        // Push to Checkmk server
        res = async {
            match push_client {
                Some(client) => push_loop(client, state.clone()).await,
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
