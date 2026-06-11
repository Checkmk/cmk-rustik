use bytes::Bytes;

use crate::error::Result;

#[derive(Debug)]
pub(crate) enum Payload {
    KubeletStatsSummary(Bytes),
    // TODO:
    // CheckmkLinuxAgent(Bytes),
}

impl Payload {
    fn metrics_cache_endpoint(&self) -> &str {
        match self {
            Self::KubeletStatsSummary(_) => "/kubelet_stats_summary",
        }
    }

    fn extract(&self) -> Bytes {
        match self {
            Self::KubeletStatsSummary(bytes) => bytes.clone(),
        }
    }

    // TODO: Unhardcode namespace name and port
    pub async fn push_to_metrics_cache(&self) -> Result<reqwest::Response> {
        let token =
            tokio::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")
                .await?;
        let response = reqwest::Client::new()
            .post(format!(
                "http://metrics-cache.checkmk-monitoring.svc.cluster.local:10050/ingress{}",
                self.metrics_cache_endpoint()
            ))
            .bearer_auth(token.trim())
            .body(self.extract())
            .send()
            .await?;
        Ok(response)
    }
}
