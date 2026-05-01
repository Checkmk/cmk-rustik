use anyhow::Result;
use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewSpec};
use kube::api::PostParams;
use kube::{Api, Client};
use std::future::Future;
use std::time::Duration;

/// Trait for validating authentication tokens.
pub trait TokenValidator: Clone + Send + Sync + 'static {
    fn validate(&self, token: &str) -> impl Future<Output = Result<TokenReview>> + Send;
}

impl TokenValidator for Client {
    async fn validate(&self, token: &str) -> Result<TokenReview> {
        let api: Api<TokenReview> = Api::all(self.clone());
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
}

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
