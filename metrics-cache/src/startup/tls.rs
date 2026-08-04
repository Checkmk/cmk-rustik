//! Bootstrap logic for metrics-cache TLS listeners.
//!
//! This module loads TLS certificates from Kubernetes Secrets and converts them
//! into server configurations. When generation is enabled and the configured
//! Secret does not exist, it creates a CA and server certificate, stores them in
//! the Secret, and handles concurrent creation by re-reading the winning Secret.
//!
//! Generating certificates in metrics-cache rather than during Helm rendering
//! gives render-only GitOps systems such as Argo CD stable manifests and avoids
//! certificate churn.
//!
//! The same logic is used independently for the pull listener and for the
//! ingestion listener used by metrics-fetcher.

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use k8s_openapi::api::core::v1::Secret;
use kube::Client;
use kube::api::Api;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
};
use std::collections::BTreeMap;
use std::time::Duration;
use time::OffsetDateTime;

use crate::cli_args::TlsConfig;
use crate::error::Result;

pub async fn resolve(client: Client, config: TlsConfig) -> anyhow::Result<Option<RustlsConfig>> {
    let Some(secret_name) = config.secret_name else {
        return Ok(None);
    };
    let secrets: Api<Secret> = Api::default_namespaced(client);
    let secret = if config.generate_if_missing {
        get_or_create_cert_secret(
            &secrets,
            &secret_name,
            &config.namespace.context("namespace required")?,
            &config.service_name.context("service name required")?,
            config.longevity,
        )
        .await?
    } else {
        secrets.get(&secret_name).await?
    };
    Ok(Some(rustls_config_from_secret(&secret).await?))
}

async fn rustls_config_from_secret(secret: &Secret) -> anyhow::Result<RustlsConfig> {
    let secret_name = secret.metadata.name.as_deref().unwrap_or("<unknown>");
    let cert = secret
        .data
        .as_ref()
        .and_then(|data| data.get("tls.crt"))
        .map(|bs| bs.0.clone())
        .with_context(|| format!("tls.crt missing from Secret {secret_name}"))?;
    let key = secret
        .data
        .as_ref()
        .and_then(|data| data.get("tls.key"))
        .map(|bs| bs.0.clone())
        .with_context(|| format!("tls.key missing from Secret {secret_name}"))?;
    Ok(RustlsConfig::from_pem(cert, key).await?)
}

struct TlsMaterial {
    ca_cert_pem: String,
    cert_pem: String,
    key_pem: String,
}

impl TlsMaterial {
    fn to_secret(&self, name: &str) -> Secret {
        Secret {
            metadata: kube::api::ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            type_: Some("kubernetes.io/tls".to_string()),
            string_data: Some(BTreeMap::from([
                ("ca.crt".to_string(), self.ca_cert_pem.clone()),
                ("tls.crt".to_string(), self.cert_pem.clone()),
                ("tls.key".to_string(), self.key_pem.clone()),
            ])),
            ..Default::default()
        }
    }
}

fn hostname(service_name: &str, namespace: &str) -> String {
    format!("{service_name}.{namespace}.svc")
}

/// Attempt to create a Kubernetes secret containing a CA certificate, a
/// certificate signed by the CA, and the certificate's key. The CA key is
/// discarded as per [`generate_tls_material()`].
///
/// If it is found that a secret exists with the same name it is returned.
/// Otherwise, the new secret is returned.
///
/// If we create a secret, we read it back from Kubernetes before returning
/// it to ensure that all fields including base64 conversions are complete.
///
/// If any other error beyond "already exists" occurs, the error is returned
/// immediately.
async fn create_cert_secret(
    secrets: &Api<Secret>,
    hostname: String,
    longevity: Duration,
    secret_name: &str,
) -> Result<Secret> {
    let dns_names = vec![hostname];
    let material = generate_tls_material(dns_names, longevity)?;
    let secret = material.to_secret(secret_name);
    let creation = secrets.create(&Default::default(), &secret).await;
    match creation {
        // If we create it successfully, query the API and return it
        Ok(_) => Ok(secrets.get(secret_name).await?),
        // If it already exists, we hit a race, try the API again
        // before we give up.
        Err(kube::Error::Api(e)) if e.is_already_exists() => Ok(secrets.get(secret_name).await?),
        Err(e) => Err(e.into()),
    }
}

/// First, attempt to find a matching Secret in the current namespace. If one
/// is found, return it immediately. Otherwise attempt to create a Secret and
/// return the new one.
///
/// Has the semantics of [`create_cert_secret()`] during creation: If during
/// creation we find that a matching secret has been created, we will read it
/// and return that (preventing potential races).
///
/// If any other error occurs that is not a "not found" error, we return it
/// immediately.
async fn get_or_create_cert_secret(
    secrets: &Api<Secret>,
    secret_name: &str,
    namespace: &str,
    service_name: &str,
    longevity: Duration,
) -> Result<Secret> {
    match secrets.get(secret_name).await {
        Ok(secret) => Ok(secret),
        Err(kube::Error::Api(e)) if e.is_not_found() => {
            let hostname = hostname(service_name, namespace);
            create_cert_secret(secrets, hostname, longevity, secret_name).await
        }
        Err(e) => Err(e.into()),
    }
}

/// Create a CA and then sign a certificate using it. Return the CA certificate,
/// the signed certificate, and the signed certificate's key, discarding the CA
/// key.
fn generate_tls_material(dns_names: Vec<String>, longevity: Duration) -> Result<TlsMaterial> {
    let not_before = OffsetDateTime::now_utc();
    let not_after = not_before + longevity;
    let ca_key = KeyPair::generate()?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params.not_before = not_before;
    ca_params.not_after = not_after;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "cmk-rustik internal CA");
    let ca_cert = ca_params.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca_params, ca_key);

    let server_key = KeyPair::generate()?;
    let mut server_params = CertificateParams::new(dns_names)?;
    server_params.not_before = not_before;
    server_params.not_after = not_after;
    server_params.distinguished_name = DistinguishedName::new();
    server_params
        .distinguished_name
        .push(DnType::CommonName, "metrics-cache");

    let server_cert = server_params.signed_by(&server_key, &issuer)?;
    Ok(TlsMaterial {
        ca_cert_pem: ca_cert.pem(),
        cert_pem: server_cert.pem(),
        key_pem: server_key.serialize_pem(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use x509_parser::{pem::parse_x509_pem, prelude::*};

    #[test]
    fn generated_tls_material_has_expected_identity_and_validity() {
        let dns_name = "metrics-cache.monitoring.svc";
        let longevity = Duration::from_secs(7 * 24 * 60 * 60);
        let material = generate_tls_material(vec![dns_name.to_string()], longevity)
            .expect("TLS material should be generated");

        let (_, ca_pem) =
            parse_x509_pem(material.ca_cert_pem.as_bytes()).expect("CA PEM should parse");
        let (_, ca_cert) =
            X509Certificate::from_der(&ca_pem.contents).expect("CA certificate should parse");
        let (_, server_pem) =
            parse_x509_pem(material.cert_pem.as_bytes()).expect("server PEM should parse");
        let (_, server_cert) = X509Certificate::from_der(&server_pem.contents)
            .expect("server certificate should parse");

        assert!(
            ca_cert
                .basic_constraints()
                .expect("CA basic constraints should parse")
                .expect("CA basic constraints should exist")
                .value
                .ca
        );
        assert_eq!(server_cert.issuer(), ca_cert.subject());

        let san = server_cert
            .subject_alternative_name()
            .expect("server SAN should parse")
            .expect("server SAN should exist");
        assert!(
            san.value.general_names.iter().any(
                |name| matches!(name, GeneralName::DNSName(candidate) if *candidate == dns_name)
            )
        );
        assert_eq!(
            server_cert.validity().not_after.timestamp()
                - server_cert.validity().not_before.timestamp(),
            i64::try_from(longevity.as_secs()).expect("test validity should fit in i64")
        );

        let secret = material.to_secret("generated-tls");
        assert_eq!(secret.type_.as_deref(), Some("kubernetes.io/tls"));
        let data = secret.string_data.expect("Secret data should exist");
        for key in ["ca.crt", "tls.crt", "tls.key"] {
            assert!(data.contains_key(key), "Secret should contain {key}");
        }
    }
}
