//! This module handles registration between metrics-cache and the Checkmk push
//! agent receiver.
//!
//! The shape roughly looks like this:
//!
//! 1. Generate a CSR (CN=UUID). UUID is random on initial registration and
//!    re-used for renewal.
//!
//! 2. Register (host must exist in Checkmk already, and one-time-token must be
//!    present).
//!
//! 3. We get back: `root_cert`, `agent_cert`, `connection_mode`, we persist
//!    the first two and the private key from step 1 in an agent-maintained
//!    Kubernetes Secret.
//!
//! ## Auto-renew (TODO)
//!
//! For renewal the steps are similar, a new CSR is made, the UUID is re-used,
//! and we authenticate with the current certificate.
//!
//! It is configurable as a command-line argument how often we check if it is
//! time to renew (and what constitutes renewal). See
//! [`crate::cli_args::CliArgs`] for current defaults.

use k8s_openapi::api::core::v1::Secret;
use kube::Client;
use kube::api::Api;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::{error, info, trace};
use uuid::Uuid;

use crate::cli_args::CliArgs;
use crate::error::{Error, Result};
use crate::push;

const CERT_SECRET: &str = "metrics-cache-cmk-push-cert";

/// Payload for push agent registration request to Checkmk server.
#[derive(Debug, Serialize)]
struct PushAgentRegistrationRequest {
    pub uuid: String,
    pub csr: String,
    pub host_name: String,
}

/// Payload for push agent registration response from Checkmk server.
#[derive(Debug, Deserialize)]
struct PushAgentRegistrationResponse {
    pub root_cert: String,
    pub agent_cert: String,
}

/// Handles the lifecycle of push registration and renewal with a Checkmk
/// server.
pub struct CheckmkPushRegistration<'a> {
    kube_client: Client,
    cli_args: &'a CliArgs,
}

impl<'a> CheckmkPushRegistration<'a> {
    pub fn new(kube_client: Client, cli_args: &'a CliArgs) -> CheckmkPushRegistration<'a> {
        Self {
            kube_client,
            cli_args,
        }
    }

    /// Attempt to fetch and return the Kubernetes Secret containing the Checkmk
    /// push agent certificate and key.
    ///
    /// As a heuristic, if the Secret is not found, it means that we have never
    /// been registered with the Checkmk server, and we should attempt to
    /// register. We indicate this by returning `Ok(None)` when the Secret is
    /// not found.
    pub async fn get_cert_secret(&self) -> Result<Option<Secret>> {
        let secrets: Api<Secret> = Api::default_namespaced(self.kube_client.clone());
        match secrets.get(CERT_SECRET).await {
            Ok(secret) => Ok(Some(secret)),
            Err(kube::Error::Api(e)) if e.code == 404 => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Generate a new CSR and private key for registration or renewal.
    ///
    /// For registration the given UUID should be random and unique. For
    /// renewal, it should be the same UUID that was used for the original
    /// registration.
    ///
    /// Returns a tuple of (csr, private_key) in PEM format.
    fn generate_registration_csr(&self, uuid: &Uuid) -> Result<(String, String)> {
        trace!("Generating key pair and CSR for registration");
        let key_pair = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec![])?;
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, uuid.to_string());
        let csr = params.serialize_request(&key_pair)?.pem()?;
        let private_key = key_pair.serialize_pem();
        trace!("Returning CSR and private key");
        Ok((csr, private_key))
    }

    /// Attempt registration with the Checkmk server
    #[tracing::instrument(
        skip(self),
        fields(
            host = %self.cli_args.cluster_host_name,
            uuid = %uuid,
            url = tracing::field::Empty,
        )
    )]
    async fn send_registration_request(
        &self,
        csr: String,
        uuid: &Uuid,
    ) -> Result<PushAgentRegistrationResponse> {
        let Some(base_url) = &self.cli_args.push_receiver else {
            error!("Push receiver URL not provided, cannot register");
            return Err(push::Error::PushMode("Push receiver URL not provided".to_string()).into());
        };
        let url = format!(
            "{}/agent-receiver/register_existing_token",
            base_url.trim_end_matches('/')
        );
        tracing::Span::current().record("url", url.as_str());
        let ott = match std::env::var("CMK_PUSH_AGENT_RECEIVER_OTT") {
            Ok(token) => token,
            Err(e) => {
                error!("CMK_PUSH_AGENT_RECEIVER_OTT not set, cannot register push agent");
                return Err(Error::EnvVar {
                    name: "CMK_PUSH_AGENT_RECEIVER_OTT".to_string(),
                    source: e,
                });
            }
        };
        let client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(true) // TOFU for register
            .build()?;

        let response = client
            .post(&url)
            .header("Authorization", format!("CMK-TOKEN {}", ott))
            .json(&PushAgentRegistrationRequest {
                uuid: uuid.to_string(),
                csr,
                host_name: self.cli_args.cluster_host_name.clone(),
            });
        let resp = response.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            error!(%status, %body, "Failed to register push agent with Checkmk server");
            return Err(push::Error::PushMode(format!(
                "Failed to register push agent with Checkmk server: {}",
                status
            ))
            .into());
        }
        resp.json::<PushAgentRegistrationResponse>()
            .await
            .map_err(|e| {
                error!("Failed to parse registration response: {}", e);
                e.into()
            })
    }

    /// First check if there is already a Secret containing the push agent
    /// certificate and key. If not, attempt to register with the Checkmk server
    /// and create the Secret.
    ///
    /// In any case, return the Secret.
    #[tracing::instrument(
        skip(self),
        fields(
            host = %self.cli_args.cluster_host_name,
            secret_name = CERT_SECRET,
            uuid = tracing::field::Empty,
        )
    )]
    pub async fn register_if_needed(&self) -> Result<Secret> {
        match self.get_cert_secret().await? {
            Some(secret) => {
                info!("Found existing push agent certificate secret, no registration needed");
                Ok(secret)
            }
            None => {
                info!("No existing push agent certificate secret found, attempting registration");
                let uuid = Uuid::new_v4();
                tracing::Span::current().record("uuid", uuid.to_string().as_str());
                let (csr, private_key) = self.generate_registration_csr(&uuid)?;
                let response = self.send_registration_request(csr, &uuid).await?;
                // Create the Secret with the received certificates and private key
                let secrets: Api<Secret> = Api::default_namespaced(self.kube_client.clone());
                let secret = Secret {
                    metadata: kube::api::ObjectMeta {
                        name: Some(CERT_SECRET.to_string()),
                        ..Default::default()
                    },
                    string_data: Some(BTreeMap::from([
                        ("root_cert".to_string(), response.root_cert),
                        ("agent_cert".to_string(), response.agent_cert),
                        ("private_key".to_string(), private_key),
                        ("uuid".to_string(), uuid.to_string()),
                    ])),
                    ..Default::default()
                };
                secrets.create(&Default::default(), &secret).await?;
                info!("Successfully registered push agent and created certificate secret");
                // The secret will only have string_data from above, we need to
                // fetch it again so we have data in the data fields, too (and a
                // full, consistent Secret object). This lets us use the Secret
                // immediately after registration.
                Ok(secrets.get(CERT_SECRET).await?)
            }
        }
    }
}
