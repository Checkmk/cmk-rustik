//! This module contains the OTel client, to export OTLP metrics to an OTel
//! collector.

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use prost::Message;

use crate::error::Result;
use crate::otel::Error;

/// A client to export OTLP metrics to an OTel collector over OTLP/http.
pub struct OtelClient {
    client: reqwest::Client,
    export_url: String,
}

impl OtelClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            export_url: format!("{}/v1/metrics", endpoint.trim_end_matches('/')),
        }
    }

    /// Send one export request to the collector.
    ///
    /// Errors on connection failure or a non-success HTTP status; the caller
    /// decides whether that is fatal (for the export loop it is not).
    pub(super) async fn export(&self, request: ExportMetricsServiceRequest) -> Result<()> {
        let response = self
            .client
            .post(&self.export_url)
            .header("content-type", "application/x-protobuf")
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(Error::Http)?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            return Err(Error::Rejected { status, body }.into());
        }
        Ok(())
    }
}
