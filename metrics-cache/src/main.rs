use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use std::io;
use std::net::SocketAddr;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use metrics_cache::cli_args::CliArgs;
use metrics_cache::handlers;
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    init_tracing(&args);
    let state = AppState::new(&args).await?;
    let app = handlers::app(state.clone());

    let push_client = match &args.push_receiver {
        Some(base_url) => {
            info!("Push receiver enabled, will push updates to Checkmk server");
            let registration = CheckmkPushRegistration::new(state.clone().client, &args);
            let secret = registration.register_if_needed().await?;
            Some(CheckmkPushClient::from_secret(base_url, &secret)?)
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

        // Push to Checkmk server
        res = async {
            match push_client {
                Some(client) => push_loop(client, state).await,
                None => std::future::pending().await,
            }
        } => {
            if let Err(e) = res {
                tracing::error!(error = %e, "push loop exited with error");
            }
            anyhow::bail!("push loop terminated unexpectedly");
        }
    }
}
