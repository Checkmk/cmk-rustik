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
//! ## Auto-renew
//!
//! For renewal the steps are similar, a new CSR is made, the UUID is re-used,
//! and we authenticate with the current certificate.
//!
//! The expiry threshold is configurable as a command-line argument. See
//! [`crate::cli_args::CliArgs`] for the current default.

use k8s_openapi::ByteString;
use k8s_openapi::api::core::v1::Secret;
use kube::Client;
use kube::api::{Api, PostParams};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::{error, info, trace, warn};
use uuid::Uuid;

use crate::cli_args::CliArgs;
use crate::error::Result;
use crate::push;
use crate::push::client::CheckmkPushClient;

const CERT_SECRET: &str = "metrics-cache-cmk-push-cert";

fn replace_secret_identity(secret: &mut Secret, agent_cert: String, private_key: String) {
    let data = secret.data.get_or_insert_default();
    data.insert(
        "agent_cert".to_string(),
        ByteString(agent_cert.into_bytes()),
    );
    data.insert(
        "private_key".to_string(),
        ByteString(private_key.into_bytes()),
    );
}

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

    /// Renew the current client identity and persist it in the certificate
    /// Secret.
    ///
    /// The replacement client is built before the Secret is updated so
    /// malformed or mismatched credentials cannot break the next restart.
    pub async fn renew(&self, client: &CheckmkPushClient) -> Result<CheckmkPushClient> {
        let base_url = self.cli_args.push_receiver.as_deref().ok_or_else(|| {
            push::Error::PushMode("Push receiver URL not provided, cannot renew".to_string())
        })?;
        let mut secret = self.get_cert_secret().await?.ok_or_else(|| {
            push::Error::PushMode("push certificate secret disappeared before renewal".to_string())
        })?;
        let secret_uuid = secret
            .data
            .as_ref()
            .and_then(|data| data.get("uuid"))
            .and_then(|uuid| std::str::from_utf8(&uuid.0).ok())
            .and_then(|uuid| Uuid::parse_str(uuid).ok())
            .ok_or_else(|| {
                push::Error::PushMode("push certificate secret contains no valid uuid".to_string())
            })?;
        if &secret_uuid != client.uuid() {
            return Err(push::Error::PushMode(
                "push certificate secret changed while metrics-cache was running".to_string(),
            )
            .into());
        }

        let (csr, private_key) = self.generate_registration_csr(&secret_uuid)?;
        let agent_cert = client.renew_certificate(&csr).await?;
        replace_secret_identity(&mut secret, agent_cert, private_key);

        let replacement = CheckmkPushClient::from_secret(base_url, &secret)?;
        let secrets: Api<Secret> = Api::default_namespaced(self.kube_client.clone());
        secrets
            .replace(CERT_SECRET, &PostParams::default(), &secret)
            .await?;
        info!(uuid = %secret_uuid, "Successfully renewed push agent certificate");
        Ok(replacement)
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
        token: &Option<String>,
    ) -> Result<PushAgentRegistrationResponse> {
        let Some(base_url) = &self.cli_args.push_receiver else {
            error!("Push receiver URL not provided, cannot register");
            return Err(push::Error::PushMode("Push receiver URL not provided".to_string()).into());
        };
        let raw_url = format!(
            "{}/agent-receiver/register_existing_token",
            base_url.trim_end_matches('/')
        );
        tracing::Span::current().record("url", &raw_url);
        let url = match reqwest::Url::parse(raw_url.as_str()) {
            Ok(url) if url.scheme() == "https" => url,
            Ok(url) => {
                return Err(push::Error::PushMode(format!(
                    "Push mode requires an 'https' URL scheme, not '{}'",
                    url.scheme()
                ))
                .into());
            }
            Err(e) => return Err(push::Error::from(e).into()),
        };

        let Some(ott) = token else {
            return Err(push::Error::PushMode(
                "Push mode was enabled but the agent is not yet registered (no stored \
                 certificate secret found) and no registration token was supplied. If you are \
                 trying to configure push mode, set your registration token in your helm \
                 push.registrationToken or create the token secret manually."
                    .to_string(),
            )
            .into());
        };

        // Check whether someone has activated the escape hatch.
        let accept_invalid_certs = self
            .cli_args
            .push_registration_insecure_skip_site_ca_verification;

        // Registration must never send the token over an unverified connection
        // unless the user explicitly opted out, which they should never do in
        // production.
        let builder = match (accept_invalid_certs, &self.cli_args.push_registration_pem) {
            (false, None) => {
                // Case 1: Not accepting invalid certs, but not given a cert
                // Intentionally do not reference the unsafe option.
                return Err(push::Error::PushMode(
                    "Push mode was enabled but the agent is not yet registered and \
                     no Checkmk site CA certificate was provided. The agent needs \
                     this to know that it is registering to the correct server and \
                     to prevent man-in-the-middle attacks. If you are trying to \
                     configure push mode, set the site CA certificate in your helm \
                     push.siteCaCertificate (on the CLI, --set-file might prove \
                     useful). The certificate can be downloaded from your Checkmk \
                     instance under Setup > Certificate overview with description \
                     \"Signing the site certificate\" and path ending \
                     \"/ssl/ca.pem\"."
                        .to_string(),
                )
                .into());
            }
            (true, Some(_)) => {
                // Case 2: Accepting invalid certs, and also given a cert
                return Err(push::Error::PushMode(
                    "Push mode was enabled with the INSECURE option \
                     push.insecureSkipSiteCaVerification in your helm values \
                     but a Checkmk site CA certificate was also supplied with \
                     push.siteCaCertificate. Exiting because we do not know \
                     which configuration is intended."
                        .to_string(),
                )
                .into());
            }
            (true, None) => {
                // Case 3: Accepting invalid certs
                warn!(
                    "INSECURE option push.insecureSkipSiteCaVerification is enabled, not \
                     validating push-agent receiver identity while registering. This \
                     configuration is NOT RECOMMENDED in production."
                );
                reqwest::ClientBuilder::new().danger_accept_invalid_certs(true)
            }
            (false, Some(pem)) => {
                // Case 4: Pinning the cert
                reqwest::ClientBuilder::new()
                    .tls_certs_only([reqwest::Certificate::from_pem(pem.as_bytes())?])
                    .danger_accept_invalid_hostnames(true)
            }
        };

        let client = builder.build()?;
        let response = client
            .post(url)
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
                let response = self
                    .send_registration_request(csr, &uuid, &self.cli_args.push_registration_ott)
                    .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_identity_preserves_other_secret_data() {
        let mut secret = Secret {
            data: Some(BTreeMap::from([
                ("agent_cert".to_string(), ByteString(b"old-cert".to_vec())),
                ("private_key".to_string(), ByteString(b"old-key".to_vec())),
                ("root_cert".to_string(), ByteString(b"root".to_vec())),
                ("uuid".to_string(), ByteString(b"uuid".to_vec())),
            ])),
            ..Default::default()
        };

        replace_secret_identity(&mut secret, "new-cert".to_string(), "new-key".to_string());

        let data = secret
            .data
            .expect("test Secret should contain identity data");
        assert_eq!(data["agent_cert"].0, b"new-cert");
        assert_eq!(data["private_key"].0, b"new-key");
        assert_eq!(data["root_cert"].0, b"root");
        assert_eq!(data["uuid"].0, b"uuid");
    }
}
