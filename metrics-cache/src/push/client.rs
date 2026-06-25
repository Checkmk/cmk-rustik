//! This module contains the push client, to push section data into a Checkmk
//! server.

use k8s_openapi::api::core::v1::Secret;
use reqwest::multipart;
use tracing::{trace, warn};

use crate::error::Result;
use crate::push::Error;

/// A client to push section data into a Checkmk server.
///
/// It assumes an existing certificate and key (to generate one and register
/// with the Checkmk server see the module [`crate::push::register`].
pub struct CheckmkPushClient {
    pub client: reqwest::Client,
    pub push_url: String,
}

impl CheckmkPushClient {
    pub fn from_secret(base_url: &str, secret: &Secret) -> Result<Self> {
        trace!("Creating CheckmkPushClient from secret");
        let key = secret
            .data
            .as_ref()
            .and_then(|data| data.get("private_key"))
            .map(|bs| String::from_utf8_lossy(&bs.0).to_string())
            .ok_or_else(|| {
                warn!("private key missing from secret");
                Error::PushMode("private key missing from secret".to_string())
            })?;
        let root_cert = secret
            .data
            .as_ref()
            .and_then(|data| data.get("root_cert"))
            .map(|bs| String::from_utf8_lossy(&bs.0).to_string())
            .ok_or_else(|| {
                warn!("root certificate missing from secret");
                Error::PushMode("root certificate missing from secret".to_string())
            })?;
        let agent_cert = secret
            .data
            .as_ref()
            .and_then(|data| data.get("agent_cert"))
            .map(|bs| String::from_utf8_lossy(&bs.0).to_string())
            .ok_or_else(|| {
                warn!("agent certificate missing from secret");
                Error::PushMode("agent certificate missing from secret".to_string())
            })?;
        let uuid = secret
            .data
            .as_ref()
            .and_then(|data| data.get("uuid"))
            .map(|bs| String::from_utf8_lossy(&bs.0).to_string())
            .ok_or_else(|| {
                warn!("uuid missing from secret");
                Error::PushMode("uuid missing from secret".to_string())
            })?;
        let cert_and_key = format!("{}\n{}", agent_cert, key);
        let client = reqwest::Client::builder()
            // TODO: We have to copy CnIsNoUuidAcceptAnyHostname from cmk-agent-ctl
            .danger_accept_invalid_hostnames(true)
            .tls_certs_only([
                reqwest::Certificate::from_pem(root_cert.as_bytes()).map_err(|e| {
                    warn!(error = %e, "failed to parse root certificate");
                    Error::PushMode("failed to parse root certificate".to_string())
                })?,
            ])
            .identity(
                reqwest::Identity::from_pem(cert_and_key.as_bytes()).map_err(|e| {
                    warn!(error = %e, "failed to parse agent certificate and key");
                    Error::PushMode("failed to parse agent certificate and key".to_string())
                })?,
            )
            .build()
            .map_err(|e| {
                warn!(error = ?e, "failed to build push-mode client");
                Error::PushMode("failed to build push-mode client".to_string())
            })?;
        let push_url = format!(
            "{}/agent-receiver/agent_data/{}",
            base_url.trim_end_matches('/'),
            uuid
        );
        Ok(Self { client, push_url })
    }

    pub(super) async fn push_section_data(&self, section_data: Vec<u8>) -> Result<()> {
        let part = multipart::Part::bytes(section_data)
            .file_name("agent_data")
            .mime_str("application/octet-stream")
            .map_err(|e| {
                warn!(error = %e, "failed to create multipart part for section data");
                Error::PushMode("failed to create multipart part for section data".to_string())
            })?;
        let form = multipart::Form::new().part("monitoring_data", part);
        let response = self
            .client
            .post(&self.push_url)
            .header("compression", "zlib")
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "failed to send section data to Checkmk server");
                Error::PushMode("failed to send section data to Checkmk server".to_string())
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            warn!(%status, %body, "Failed to push section data to Checkmk server");
            return Err(Error::PushMode(format!(
                "Failed to push section data to Checkmk server: {}",
                status
            ))
            .into());
        }
        Ok(())
    }
}
