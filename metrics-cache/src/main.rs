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
    if let Some(base_url) = &args.push_receiver {
        info!("Push receiver enabled, attempting registration with Checkmk server");
        let registration = CheckmkPushRegistration::new(state.clone().client, &args);
        let secret = registration.register_if_needed().await?;
        let client = CheckmkPushClient::from_secret(base_url, &secret)?;
        tokio::select! {
            res = push_loop(client, state) => {
                if let Err(e) = res {
                    tracing::error!(error = %e, "push loop exited with error");
                }
                anyhow::bail!("push loop terminated unexpectedly");
            }
            res = bind(&args, app) => {
                if let Err(e) = res {
                    tracing::error!(error = %e, "Axum server exited with error");
                }
                anyhow::bail!("Axum server terminated unexpectedly");
            }
        }
    } else {
        bind(&args, app).await?;
    }
    Ok(())
}
