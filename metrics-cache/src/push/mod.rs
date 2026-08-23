pub mod client;
pub mod register;
mod renew;
pub mod server_cert_verifier;

use anyhow::Context;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use kube::runtime::reflector::store::WriterDropped;
use thiserror::Error;
use tokio::time;
use tokio::time::Duration;
use tracing::{debug, error};

use crate::auth::kubernetes::TokenValidator;
use crate::piggyback::emit_all;
use crate::push::client::CheckmkPushClient;
use crate::section::writeable::frame;
use crate::snapshot::Snapshot;
use crate::state::AppState;

#[derive(Error, Debug)]
pub enum Error {
    #[error("rcgen error")]
    Rcgen(#[from] rcgen::Error),
    #[error("push-mode error: {0}")]
    PushMode(String),
    #[error("failed to configure push-mode TLS")]
    TlsClientConfig(#[source] server_cert_verifier::ClientConfigError),
    #[error(
        "push mode is enabled but no registration token was given; set push.registrationToken in \
         helm values or create the identity secret manually"
    )]
    MissingRegistrationToken,
    #[error("Failed to parse push-mode URL: {0}")]
    UrlParseError(#[from] url::ParseError),
}

/// Generate, compress, and push sections from the current state.
///
/// This is what actually generates the sections, compresses them, and pushes
/// them to Checkmk using the push client. It runs once per push interval,
/// called via [`push_loop()`], and builds a [`Snapshot`], emits all sections,
/// zlib-compresses them, and pushes via the [`CheckmkPushClient`].
async fn push_cycle(
    client: &CheckmkPushClient,
    state: &AppState<impl TokenValidator>,
) -> anyhow::Result<()> {
    let snap = Snapshot::new(
        state.stores.clone(),
        state.kubelet_stats_summary_cache.clone(),
        state.kubelet_health_cache.clone(),
        state.system_agent_cache.clone(),
        state.api_health_receiver.clone(),
    );
    let sections = emit_all(&snap, &state.host_settings);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    frame(&mut encoder, sections).context("framing push data")?;
    let encoded = encoder.finish().context("finishing zlib encoding")?;
    client.push_section_data(encoded).await?;
    Ok(())
}

/// Run the push loop forever.
///
/// Every push interval, [`push_cycle()`] is called to generate sections and
/// send them to the Checkmk server.
///
/// Waits until all stores are ready before doing its initial push and looping.
///
/// Owns the [`CheckmkPushClient`] for the lifetime of the loop.
pub async fn push_loop(
    client: CheckmkPushClient,
    state: AppState<impl TokenValidator>,
    push_interval: Duration,
) -> Result<(), WriterDropped> {
    state.stores.wait_until_all_ready().await?;
    let mut interval = time::interval(push_interval);
    loop {
        interval.tick().await; // note: The very first tick() is no-op
        match push_cycle(&client, &state).await {
            Ok(_) => debug!("Successfully pushed metrics to Checkmk server"),
            Err(e) => error!(error = ?e, "Failed to push metrics to Checkmk server"),
        }
    }
}
