//! TLS verification for pushing to the Checkmk push receiver.
//!
//! The site CA signs both the receiver's server certificate and agent client
//! certificates. Rejecting a UUID common name prevents accepting an agent
//! certificate as a receiver certificate. Hostname verification deliberately
//! uses the certificate's common name because the configured receiver address
//! may differ from the certificate identity.
//!
//! This is the verifying-policy subset of `ServerCertChecker` from
//! `packages/cmk-agent-ctl/src/certs.rs` in the Checkmk monorepo.

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{
    DangerousClientConfigBuilder, HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use std::sync::Arc;
use thiserror::Error;
use x509_parser::prelude::FromDer;

#[derive(Debug, Error)]
pub enum ClientConfigError {
    #[error("no default rustls crypto provider installed")]
    MissingCryptoProvider,
    #[error("failed to read {input} PEM")]
    ReadPem {
        input: &'static str,
        #[source]
        source: rustls::pki_types::pem::Error,
    },
    #[error("{0} does not contain a certificate PEM block")]
    MissingCertificate(&'static str),
    #[error("invalid root certificate")]
    InvalidRootCertificate(#[source] rustls::Error),
    #[error("failed to create server certificate verifier")]
    ServerCertVerifier(#[source] rustls::client::VerifierBuilderError),
    #[error("agent certificate and private key are invalid")]
    InvalidClientIdentity(#[source] rustls::Error),
}

#[derive(Debug)]
struct DisallowUuidCn {
    crypto_provider: Arc<CryptoProvider>,
    verifier: Arc<dyn ServerCertVerifier>,
}

impl DisallowUuidCn {
    fn new(
        roots: RootCertStore,
        crypto_provider: &Arc<CryptoProvider>,
    ) -> Result<Self, rustls::client::VerifierBuilderError> {
        Ok(Self {
            crypto_provider: crypto_provider.clone(),
            verifier: WebPkiServerVerifier::builder_with_provider(
                Arc::new(roots),
                crypto_provider.clone(),
            )
            .build()?,
        })
    }
}

fn common_name(certificate: &CertificateDer<'_>) -> Result<String, rustls::Error> {
    let (_remaining, certificate) =
        x509_parser::certificate::X509Certificate::from_der(certificate.as_ref())
            .map_err(|_| rustls::Error::InvalidCertificate(CertificateError::BadEncoding))?;
    let common_names = certificate
        .subject()
        .iter_common_name()
        .map(|name| {
            name.as_str()
                .map(str::to_owned)
                .map_err(|error| rustls::Error::General(format!("Failed to parse CN: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    match common_names.as_slice() {
        [common_name] => Ok(common_name.clone()),
        _ => Err(rustls::Error::General(format!(
            "Expected exactly one CN in server certificate, found: {}",
            common_names.join(", ")
        ))),
    }
}

impl ServerCertVerifier for DisallowUuidCn {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let common_name = common_name(end_entity)?;
        if uuid::Uuid::parse_str(&common_name).is_ok() {
            return Err(rustls::Error::General(format!(
                "CN in server certificate is a valid UUID: {common_name}"
            )));
        }
        let verified_name = ServerName::try_from(common_name).map_err(|error| {
            rustls::Error::General(format!(
                "CN in server certificate cannot be used as server name: {error}"
            ))
        })?;

        self.verifier.verify_server_cert(
            end_entity,
            intermediates,
            &verified_name,
            ocsp_response,
            now,
        )
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.crypto_provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.crypto_provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.crypto_provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn certificates(
    input: &'static str,
    pem: &str,
) -> Result<Vec<CertificateDer<'static>>, ClientConfigError> {
    let certificates = CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| ClientConfigError::ReadPem { input, source })?;
    if certificates.is_empty() {
        return Err(ClientConfigError::MissingCertificate(input));
    }
    Ok(certificates)
}

fn root_cert_store(root_cert_pem: &str) -> Result<RootCertStore, ClientConfigError> {
    let certificates = certificates("root certificate", root_cert_pem)?;

    let mut roots = RootCertStore::empty();
    for certificate in certificates {
        roots
            .add(certificate)
            .map_err(ClientConfigError::InvalidRootCertificate)?;
    }
    Ok(roots)
}

pub(super) fn client_config(
    root_cert_pem: &str,
    agent_cert_pem: &str,
    private_key_pem: &str,
) -> Result<ClientConfig, ClientConfigError> {
    let crypto_provider =
        CryptoProvider::get_default().ok_or(ClientConfigError::MissingCryptoProvider)?;
    let verifier = DisallowUuidCn::new(root_cert_store(root_cert_pem)?, crypto_provider)
        .map_err(ClientConfigError::ServerCertVerifier)?;
    let certificate_chain = certificates("agent certificate", agent_cert_pem)?;
    let private_key =
        PrivateKeyDer::from_pem_slice(private_key_pem.as_bytes()).map_err(|source| {
            ClientConfigError::ReadPem {
                input: "private key",
                source,
            }
        })?;

    DangerousClientConfigBuilder {
        cfg: ClientConfig::builder(),
    }
    .with_custom_certificate_verifier(Arc::new(verifier))
    .with_client_auth_cert(certificate_chain, private_key)
    .map_err(ClientConfigError::InvalidClientIdentity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    };

    fn verifier_and_certificate(
        common_name: &str,
        subject_alt_name: &str,
    ) -> (DisallowUuidCn, CertificateDer<'static>) {
        let ca_key = KeyPair::generate().expect("CA key should generate");
        let mut ca_params =
            CertificateParams::new(Vec::<String>::new()).expect("CA parameters should be valid");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_certificate = ca_params
            .self_signed(&ca_key)
            .expect("CA certificate should generate");
        let issuer = Issuer::new(ca_params, ca_key);

        let server_key = KeyPair::generate().expect("server key should generate");
        let mut server_params = CertificateParams::new(vec![subject_alt_name.to_string()])
            .expect("server parameters should be valid");
        server_params.distinguished_name = DistinguishedName::new();
        server_params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        let server_certificate = server_params
            .signed_by(&server_key, &issuer)
            .expect("server certificate should generate");

        let mut roots = RootCertStore::empty();
        roots
            .add(ca_certificate.der().clone())
            .expect("CA should be accepted as a trust anchor");
        let verifier =
            DisallowUuidCn::new(roots, &Arc::new(rustls::crypto::ring::default_provider()))
                .expect("verifier should build");
        (verifier, server_certificate.der().clone())
    }

    #[test]
    fn accepts_non_uuid_cn_independently_of_requested_hostname() {
        let (verifier, certificate) =
            verifier_and_certificate("checkmk.example", "checkmk.example");
        assert!(
            verifier
                .verify_server_cert(
                    &certificate,
                    &[],
                    &ServerName::try_from("different.example").expect("valid server name"),
                    &[],
                    UnixTime::now(),
                )
                .is_ok()
        );
    }

    #[test]
    fn rejects_uuid_cn() {
        let uuid = "cf771eeb-b666-4673-95c9-683960fb2939";
        let (verifier, certificate) = verifier_and_certificate(uuid, uuid);
        let error = verifier
            .verify_server_cert(
                &certificate,
                &[],
                &ServerName::try_from("different.example").expect("valid server name"),
                &[],
                UnixTime::now(),
            )
            .expect_err("UUID common name should be rejected");
        assert_eq!(
            error.to_string(),
            format!("unexpected error: CN in server certificate is a valid UUID: {uuid}")
        );
    }

    #[test]
    fn rejects_non_uuid_cn_signed_by_untrusted_ca() {
        let (verifier, _) = verifier_and_certificate("checkmk.example", "checkmk.example");
        let (_, untrusted_certificate) =
            verifier_and_certificate("checkmk.example", "checkmk.example");

        assert!(
            verifier
                .verify_server_cert(
                    &untrusted_certificate,
                    &[],
                    &ServerName::try_from("different.example").expect("valid server name"),
                    &[],
                    UnixTime::now(),
                )
                .is_err()
        );
    }

    #[test]
    fn rejects_cn_not_in_subject_alt_names() {
        let (verifier, certificate) = verifier_and_certificate("checkmk.example", "other.example");

        assert!(
            verifier
                .verify_server_cert(
                    &certificate,
                    &[],
                    &ServerName::try_from("different.example").expect("valid server name"),
                    &[],
                    UnixTime::now(),
                )
                .is_err()
        );
    }
}
