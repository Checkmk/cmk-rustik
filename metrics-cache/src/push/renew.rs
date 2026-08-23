use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use tracing::error;
use uuid::Uuid;

use crate::error::Result;
use crate::push::Error;
use crate::push::client::CheckmkPushClient;
use crate::push::server_cert_verifier::PushTlsConfig;

const MAX_RENEWAL_RETRY_INTERVAL: Duration = Duration::from_hours(24);

#[derive(Serialize)]
struct RenewCertificateRequest<'a> {
    csr: &'a str,
}

#[derive(Deserialize)]
struct RenewCertificateResponse {
    agent_cert: String,
}

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

impl CheckmkPushClient {
    /// Request a replacement certificate using the current mTLS identity.
    pub(super) async fn renew_certificate(&self, csr: &str) -> Result<String> {
        let response = self
            .client
            .post(&self.renewal.renew_url)
            .json(&RenewCertificateRequest { csr })
            .send()
            .await
            .map_err(|error| {
                error!(?error, "failed to request certificate renewal");
                Error::PushMode("failed to request certificate renewal".to_string())
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            error!(%status, %body, "Failed to renew push agent certificate");
            return Err(Error::PushMode(format!(
                "Failed to renew push agent certificate: {status}"
            ))
            .into());
        }
        let response = response
            .json::<RenewCertificateResponse>()
            .await
            .map_err(|error| {
                error!(?error, "failed to parse certificate renewal response");
                error
            })?;
        Ok(response.agent_cert)
    }

    pub(super) fn uuid(&self) -> &Uuid {
        &self.renewal.uuid
    }

    /// Record and begin a renewal attempt when the certificate requires one.
    pub(super) fn begin_renewal_attempt(&mut self, expiry_window: Duration) -> bool {
        let now = SystemTime::now();
        if !must_attempt_renewal(
            self.renewal.last_attempt,
            now,
            self.renewal.valid_from,
            self.renewal.expires_at,
            expiry_window,
        ) {
            return false;
        }
        self.renewal.last_attempt = Some(now);
        true
    }

    /// Replace the mTLS identity while retaining the renewal retry state.
    pub(super) fn replace_identity(&mut self, mut replacement: Self) {
        replacement.renewal.last_attempt = self.renewal.last_attempt;
        *self = replacement;
    }
}

/// Determine whether a currently valid certificate should be renewed.
fn must_attempt_renewal(
    last_attempt: Option<SystemTime>,
    now: SystemTime,
    valid_from: SystemTime,
    expires_at: SystemTime,
    expiry_window: Duration,
) -> bool {
    if now < valid_from {
        return false;
    }
    let Ok(remaining) = expires_at.duration_since(now) else {
        return false;
    };
    if remaining.is_zero() || remaining > expiry_window {
        return false;
    }
    let retry_interval = std::cmp::min(MAX_RENEWAL_RETRY_INTERVAL, remaining / 2);
    last_attempt.is_none_or(|last| now.duration_since(last).unwrap_or_default() >= retry_interval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renewal_is_required_for_certificate_expiry() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_hours(1_000);
        let expiry_window = Duration::from_hours(24 * 45);
        let cases = [
            (
                "healthy certificate",
                now,
                now + Duration::from_hours(24 * 60),
                false,
            ),
            (
                "certificate near expiry",
                now,
                now + Duration::from_hours(24 * 44),
                true,
            ),
            (
                "certificate at renewal threshold",
                now,
                now + expiry_window,
                true,
            ),
            ("expired certificate", now, now, false),
            (
                "not-yet-valid certificate",
                now + Duration::from_hours(1),
                now + Duration::from_hours(24 * 44),
                false,
            ),
        ];

        for (case, valid_from, expires_at, expected) in cases {
            assert_eq!(
                must_attempt_renewal(None, now, valid_from, expires_at, expiry_window),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn renewal_retries_before_certificate_expiry() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_hours(1_000);
        let expiry_window = Duration::from_hours(24 * 45);
        let far_expiry = now + Duration::from_hours(24 * 30);

        for (case, last_attempt, elapsed, expected) in [
            ("first attempt", None, Duration::ZERO, true),
            (
                "daily cooldown for certificates with ample validity",
                Some(now),
                Duration::from_hours(23),
                false,
            ),
            (
                "daily cooldown elapsed",
                Some(now),
                Duration::from_hours(24),
                true,
            ),
        ] {
            assert_eq!(
                must_attempt_renewal(last_attempt, now + elapsed, now, far_expiry, expiry_window,),
                expected,
                "{case}"
            );
        }

        let expires_at = now + Duration::from_hours(12);
        assert!(!must_attempt_renewal(
            Some(now),
            now + Duration::from_hours(3),
            now,
            expires_at,
            expiry_window,
        ));
        assert!(must_attempt_renewal(
            Some(now),
            now + Duration::from_hours(6),
            now,
            expires_at,
            expiry_window,
        ));
    }
}
