//! This module contains the OTel client, to export OTLP metrics to an OTel
//! collector.

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use prost::Message;
use tracing::error;

use crate::error::Result;
use crate::otel::Error;

/// Basic-auth credentials for the OTel collector.
pub struct BasicAuth {
    username: String,
    password: Option<String>,
}

impl BasicAuth {
    /// Resolve the configured username and password, treating empty values as
    /// unset. A password without a username is an error.
    pub fn resolve(username: Option<&str>, password: Option<&str>) -> Result<Option<Self>> {
        let username = username.filter(|value| !value.is_empty());
        let password = password.filter(|value| !value.is_empty());
        match (username, password) {
            (Some(username), password) => Ok(Some(Self {
                username: username.to_string(),
                password: password.map(str::to_string),
            })),
            (None, Some(_)) => {
                error!("an OTel basic-auth password was configured without a username");
                Err(Error::MissingUsername.into())
            }
            (None, None) => Ok(None),
        }
    }
}

/// A client to export OTLP metrics to an OTel collector over OTLP/http.
pub struct OtelClient {
    client: reqwest::Client,
    export_url: String,
    basic_auth: Option<BasicAuth>,
}

impl OtelClient {
    pub fn new(endpoint: &str, basic_auth: Option<BasicAuth>) -> Self {
        Self {
            client: reqwest::Client::new(),
            export_url: format!("{}/v1/metrics", endpoint.trim_end_matches('/')),
            basic_auth,
        }
    }

    /// Send one export request to the collector.
    ///
    /// Errors on connection failure or a non-success HTTP status; the caller
    /// decides whether that is fatal (for the export loop it is not).
    pub(super) async fn export(&self, request: ExportMetricsServiceRequest) -> Result<()> {
        let mut builder = self
            .client
            .post(&self.export_url)
            .header("content-type", "application/x-protobuf")
            .body(request.encode_to_vec());
        if let Some(auth) = &self.basic_auth {
            builder = builder.basic_auth(&auth.username, auth.password.as_ref());
        }
        let response = builder.send().await.map_err(Error::Http)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_treats_empty_values_as_unset() {
        for (username, password, expected) in [
            (
                Some("bob"),
                Some("McBobberson"),
                Some(("bob", Some("McBobberson"))),
            ),
            (Some("bob"), None, Some(("bob", None))),
            (Some("bob"), Some(""), Some(("bob", None))),
            (None, None, None),
            (Some(""), Some(""), None),
        ] {
            let resolved = BasicAuth::resolve(username, password)
                .expect("a username with any password is valid");
            assert_eq!(
                resolved.map(|auth| (auth.username, auth.password)),
                expected.map(|(username, password)| (
                    username.to_string(),
                    password.map(str::to_string)
                ))
            );
        }
    }

    #[test]
    fn basic_auth_rejects_a_password_without_a_username() {
        for username in [None, Some("")] {
            assert!(BasicAuth::resolve(username, Some("McBobberson")).is_err());
        }
    }
}
