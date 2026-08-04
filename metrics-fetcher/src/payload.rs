use bytes::Bytes;
use reqwest::Client;
use tracing::{debug, trace, warn};

use crate::error::Result;

#[derive(Debug)]
pub(crate) enum Payload {
    KubeletStatsSummary(Bytes),
    CheckmkLinuxAgent { node_name: String, body: Bytes },
}

impl Payload {
    fn metrics_cache_endpoint(&self) -> String {
        match self {
            Self::KubeletStatsSummary(_) => "/kubelet_stats_summary".to_string(),
            Self::CheckmkLinuxAgent { node_name, .. } => format!("/linux_agent/{node_name}"),
        }
    }

    fn content_type(&self) -> &'static str {
        match self {
            Self::KubeletStatsSummary(_) => "application/json",
            Self::CheckmkLinuxAgent { .. } => "text/plain; charset=utf-8",
        }
    }

    fn extract(&self) -> Bytes {
        match self {
            Self::KubeletStatsSummary(bytes) => bytes.clone(),
            Self::CheckmkLinuxAgent { body, .. } => body.clone(),
        }
    }

    pub async fn push_to_metrics_cache(
        &self,
        namespace: &str,
        service: &str,
        port: u16,
        client: Client,
    ) -> Result<reqwest::Response> {
        trace!("reading kubelet token");
        let token =
            tokio::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")
                .await?;
        let url = format!(
            "http://{service}.{namespace}.svc.cluster.local:{port}/ingest{}",
            self.metrics_cache_endpoint()
        );
        debug!(url = %url, "relaying payload to metrics-cache");
        trace!(
            payload = ?self,
            "payload being sent"
        );
        let response = client
            .post(&url)
            .bearer_auth(token.trim())
            .body(self.extract())
            .header("content-type", self.content_type())
            .send()
            .await?;
        if response.status().is_success() {
            debug!(status = %response.status(), "payload accepted by metrics-cache");
        } else {
            warn!(status = ?response, "payload rejected by metrics-cache");
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kubelet_stats_summary_endpoint_and_content_type() {
        let payload = Payload::KubeletStatsSummary(Bytes::from_static(b"{}"));
        assert_eq!(payload.metrics_cache_endpoint(), "/kubelet_stats_summary");
        assert_eq!(payload.content_type(), "application/json");
        assert_eq!(payload.extract(), Bytes::from_static(b"{}"));
    }

    #[test]
    fn checkmk_linux_agent_endpoint_and_content_type() {
        for node_name in ["node-1", "node-with-dashes.example.com"] {
            let payload = Payload::CheckmkLinuxAgent {
                node_name: node_name.to_string(),
                body: Bytes::from_static(b"<<<check_mk>>>\n"),
            };
            assert_eq!(
                payload.metrics_cache_endpoint(),
                format!("/linux_agent/{node_name}")
            );
            assert_eq!(payload.content_type(), "text/plain; charset=utf-8");
            assert_eq!(payload.extract(), Bytes::from_static(b"<<<check_mk>>>\n"));
        }
    }
}
