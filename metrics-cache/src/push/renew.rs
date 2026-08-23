use std::time::SystemTime;
use uuid::Uuid;

use crate::push::server_cert_verifier::PushTlsConfig;

/// Identity and certificate timing state used for automatic renewal.
///
/// Certificate validity and retry attempts use wall-clock timestamps to match
/// the validity checks performed during TLS handshakes.
pub(super) struct RenewalState {
    renew_url: String,
    uuid: Uuid,
    valid_from: SystemTime,
    expires_at: SystemTime,
    last_attempt: Option<SystemTime>,
}

impl RenewalState {
    /// Build the renewal endpoint and capture the certificate validity.
    pub(super) fn new(base_url: &str, uuid: Uuid, tls: &PushTlsConfig) -> Self {
        Self {
            renew_url: format!(
                "{}/agent-receiver/renew_certificate/{uuid}",
                base_url.trim_end_matches('/')
            ),
            uuid,
            valid_from: tls.certificate_validity.not_before,
            expires_at: tls.certificate_validity.not_after,
            last_attempt: None,
        }
    }
}
