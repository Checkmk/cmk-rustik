//! OTLP metrics export, shaped like the collector's kubeletstats receiver.
//!
//! Built directly on the OTLP protobuf types rather than the OpenTelemetry
//! SDK: the kubeletstats shape requires one `Resource` per pod / container,
//! but the SDK fixes exactly one `Resource` per provider. The protocol is
//! multi-resource by design, the SDK is not.
//!
//! Layout: [`collect`] is the domain half (collects the data and prepares it
//! for sending), [`wire`] is the OTLP mapping (how to build up the Protobuf
//! pyramid from struct instances built in [`collect`]), and [`client`] talks to
//! the collector and sends the data.
//!
//! The rough sketch of data flow is: The raw data comes in via the
//! `metrics-fetchers` (DaemonSet, one instance per node). `metrics-fetcher`
//! periodically POSTs its collected kubelet metric data to `metrics-cache`
//! (this crate) whose Axum handler stores it in a Moka (in-memory) cache stored
//! in the [`AppState`]. This cache constitutes the primary (and, with very few
//! exceptions, the _only_) input for the `otel` modules to function.
//!
//! In `main.rs`, an [`OtelClient`] is constructed and passed to [`otel_loop()`]
//! along with a copy of the [`AppState`].
//!
//! Every interval, the loop calls [`collect::collect_entities()`], which walks
//! the pods of all stored kubelet metrics (for all nodes) and tallies up the
//! container-level metrics and pod-level metrics (sums of the respective
//! container metrics), creating [`wire::KubeGauge`]s stored in
//! [`wire::KubeEntity`] instances along the way.
//!
//! These types have `From` implementations in [`wire`] that convert them into
//! the appropriate pieces of the Protobuf hierarchy for OpenTelemetry.
//! Ultimately, using _this_ module's [`to_request()`], the result is wrapped in
//! the final layer, ready to be encoded and shipped by [`OtelClient::export()`].

pub mod client;
mod collect;
mod wire;

/// Errors from talking to the OTel collector.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("http (reqwest) error")]
    Http(#[from] reqwest::Error),
    #[error("collector rejected export: {status}: {body}")]
    Rejected {
        status: reqwest::StatusCode,
        body: String,
    },
}

use std::time::Duration;
use tokio::time;
use tracing::{debug, error};

use crate::auth::kubernetes::TokenValidator;
use crate::otel::client::OtelClient;
use crate::otel::collect::collect_entities;
use crate::otel::wire::KubeEntity;
use crate::state::AppState;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::metrics::v1::ResourceMetrics;

/// Wrap the entities into the OTLP request envelope (one request per export).
fn to_request(entities: Vec<KubeEntity>) -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest {
        resource_metrics: entities.into_iter().map(ResourceMetrics::from).collect(),
    }
}

/// Run the OTel export loop forever.
///
/// Every export interval, the kubelet stats cache is collected into entities
/// and sent to the OTel collector server. A failed export is logged and the
/// cycle skipped; it never takes the rest of the process down.
///
/// Depends entirely on the kubelet stats cache in `state` and does effectively
/// nothing if it is not yet populated.
pub async fn otel_loop(client: OtelClient, state: AppState<impl TokenValidator>) {
    let mut interval = time::interval(Duration::from_secs(60)); // TODO: Unhardcode
    loop {
        interval.tick().await; // note: The very first tick() is no-op
        let entities = collect_entities(&state);
        let resources = entities.len();
        match client.export(to_request(entities)).await {
            Ok(()) => debug!(resources, "exported metrics to OTel collector"),
            Err(err) => error!(%err, "failed to export metrics to OTel collector"),
        }
    }
}
