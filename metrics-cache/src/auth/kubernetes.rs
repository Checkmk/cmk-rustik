use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewSpec};
use kube::api::PostParams;
use kube::{Api, Client};
use std::future::Future;

/// Trait for validating authentication tokens.
pub trait TokenValidator: Clone + Send + Sync + 'static {
    type Error: Send + Sync + 'static;
    fn validate(
        &self,
        token: &str,
    ) -> impl Future<Output = Result<TokenReview, Self::Error>> + Send;
}

impl TokenValidator for Client {
    type Error = kube::Error;
    async fn validate(&self, token: &str) -> kube::Result<TokenReview> {
        let api: Api<TokenReview> = Api::all(self.clone());
        let tr = TokenReview {
            spec: TokenReviewSpec {
                token: token.to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        api.create(&PostParams::default(), &tr).await
    }
}
