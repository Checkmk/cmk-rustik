use anyhow::Result;
use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewSpec};
use kube::api::PostParams;
use kube::{Api, Client};
use std::time::Duration;

/// Generate a Kubernetes client config with overrides from CLI arguments for
/// connection and read timeouts. Will attempt to read from `$KUBECONFIG` or
/// `~/.kube/config` first, then fall back to trying to read from the
/// environment variables `KUBERNETES_SERVICE_HOST` and assuming a token in
/// `/var/run/secrets/kubernetes.io/serviceaccount/`. This allows for testing
/// running outside of a cluster as well as the normal in-cluster deployment.
pub async fn kube_client(connect_timeout: Duration, read_timeout: Duration) -> Result<Client> {
    let mut config = kube::Config::infer().await?;
    config.connect_timeout = Some(connect_timeout);
    config.read_timeout = Some(read_timeout);

    Ok(kube::Client::try_from(config)?)
}

/// Request Kubernetes to review/validate the given token.
///
/// Importantly: This does NOT validate against any local whitelist, callers are
/// expected to do that independently.
pub async fn validate_token_against_kube(client: Client, token: &str) -> Result<TokenReview> {
    let api: Api<TokenReview> = Api::all(client);
    let tr = TokenReview {
        spec: TokenReviewSpec {
            token: Some(token.to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let response = api.create(&PostParams::default(), &tr).await?;
    Ok(response)
}
