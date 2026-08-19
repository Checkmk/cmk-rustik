//! Authentication machinery for the pull special agent to work.
//!
//! The pull agent will send us `Authorization: Bearer <shared_secret>`
//!
//! The value of the shared secret is passed in via an environment variable,
//! `CMK_PULL_SHARED_SECRET`. In a typical deployment, it lives in a Kubernetes
//! secret and the env var is bound from that. It is read by clap in
//! [`crate::cli_args::CliArgs`] and can also be overridden via a CLI flag,
//! though this is unsafe (since the process information would leak it) and we
//! do not do this in the Helm chart.

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;

/// Configuration for the pull-agent middleware.
///
/// Right now, includes the shared secret; in theory in the future could also
/// include things like an IP whitelist.
#[derive(Clone)]
pub struct PullAgentMiddlewareConfig {
    /// Defines if authentication should be required for pull endpoints at all
    pub auth_enabled: bool,
    /// The shared secret that we expect to be sent by the Checkmk special agent
    /// for authentication. We expect to find this in the `Authorization` header
    /// after `Bearer`
    pub shared_secret: Option<String>,
}

impl Default for PullAgentMiddlewareConfig {
    /// Defaults closed: auth enabled with no configured secret, so
    /// [`authorized`] rejects every request. This is only meant as a
    /// placeholder for tests that don't exercise pull-agent auth at all; if it
    /// were ever reached in real code, it fails safe instead of leaving the
    /// endpoint open.
    fn default() -> Self {
        Self {
            auth_enabled: true,
            shared_secret: None,
        }
    }
}

/// Perform authentication, specifically for pull-agent endpoints.
///
/// If `config.shared_secret` is `None` or empty, we _REJECT_ all requests.
/// `None` means pull mode is not configured at all; an empty value is what a
/// misconfigured secret (wrong key, empty env var) produces, so it must never
/// mean "public". The only way to get an unauthenticated pull endpoint is the
/// explicit `--disable-pull-authentication` flag (`auth_enabled: false`).
///
/// Otherwise we only permit the request if its `Authorization: Bearer <...>`
/// value matches the shared secret value.
pub(crate) async fn authenticate(
    State(config): State<PullAgentMiddlewareConfig>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());
    if authorized(&config, authorization) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// The pure authentication decision: is a request carrying this
/// `Authorization` header value allowed through under this config?
fn authorized(config: &PullAgentMiddlewareConfig, authorization: Option<&str>) -> bool {
    if !config.auth_enabled {
        return true;
    }
    match config.shared_secret.as_deref() {
        // Unconfigured or misconfigured-empty: fail closed.
        None | Some("") => false,
        Some(configured_secret) => {
            let Some(received_secret) =
                authorization.and_then(|value| value.strip_prefix("Bearer "))
            else {
                return false;
            };
            // Constant-time comparison so response timing does not leak
            // how much of the secret matched.
            received_secret
                .as_bytes()
                .ct_eq(configured_secret.as_bytes())
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(auth_enabled: bool, shared_secret: Option<&str>) -> PullAgentMiddlewareConfig {
        PullAgentMiddlewareConfig {
            auth_enabled,
            shared_secret: shared_secret.map(String::from),
        }
    }

    #[test]
    fn authorized_policies() {
        // The happy path, and the only accepted header shape.
        assert!(authorized(&config(true, Some("s3c")), Some("Bearer s3c")));
        assert!(!authorized(&config(true, Some("s3c")), Some("Bearer nope")));
        assert!(!authorized(&config(true, Some("s3c")), Some("Secret s3c")));
        assert!(!authorized(&config(true, Some("s3c")), Some("bearer s3c")));
        assert!(!authorized(&config(true, Some("s3c")), Some("s3c")));
        assert!(!authorized(&config(true, Some("s3c")), Some("")));
        assert!(!authorized(&config(true, Some("s3c")), None));

        // No secret configured: fail closed
        assert!(!authorized(&config(true, None), Some("Bearer s3c")));

        // Empty secret never matches, not even an empty presented one.
        assert!(!authorized(&config(true, Some("")), Some("Bearer ")));

        // Auth explicitly disabled: everything passes.
        assert!(authorized(&config(false, None), None));
    }
}
