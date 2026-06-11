use bytes::Bytes;
use tracing::{debug, trace, warn};

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
        trace!("reading kubelet token");
        let token =
            tokio::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")
                .await?;
        let url = format!(
            "http://metrics-cache.checkmk-monitoring.svc.cluster.local:10050/ingress{}",
            self.metrics_cache_endpoint()
        );
        debug!(url = %url, "relaying payload to metrics-cache");
        trace!(
            payload = ?self,
            "payload being sent"
        );
        let response = reqwest::Client::new()
            .post(&url)
            .bearer_auth(token.trim())
            .body(self.extract())
            .send()
            .await?;
        if response.status().is_success() {
            trace!(status = %response.status(), "payload accepted by metrics-cache");
        } else {
            warn!(status = ?response, "payload rejected by metrics-cache");
        }
        Ok(response)
    }
}
