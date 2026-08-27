use bytes::Bytes;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, trace, warn};

use crate::error::Result;

#[derive(Debug)]
pub(crate) enum Payload {
    KubeletStatsSummary(Bytes),
    KubeletHealth { node_name: String, body: Bytes },
    CheckmkLinuxAgent { node_name: String, body: Bytes },
}

impl Payload {
    fn metrics_cache_endpoint(&self) -> String {
        match self {
            Self::KubeletStatsSummary(_) => "/kubelet_stats_summary".to_string(),
            Self::KubeletHealth { node_name, .. } => format!("/kubelet_health/{node_name}"),
            Self::CheckmkLinuxAgent { node_name, .. } => format!("/system_agent/{node_name}"),
        }
    }

    fn content_type(&self) -> &'static str {
        match self {
            Self::KubeletStatsSummary(_) => "application/json",
            Self::KubeletHealth { .. } => "application/json",
            // Not text/plain: a patched image's plugin can make check_mk_agent
            // output non-UTF-8, even non-textual, so we don't claim otherwise.
            Self::CheckmkLinuxAgent { .. } => "application/octet-stream",
        }
    }

    fn extract(&self) -> Bytes {
        match self {
            Self::KubeletStatsSummary(bytes) => bytes.clone(),
            Self::KubeletHealth { body, .. } => body.clone(),
            Self::CheckmkLinuxAgent { body, .. } => body.clone(),
        }
    }

    pub async fn push_to_metrics_cache(
        &self,
        namespace: &str,
        service: &str,
        ca_cert_file: &Option<String>,
        port: u16,
        client: Client,
        scrape_duration: Duration,
    ) -> Result<reqwest::Response> {
        trace!("reading kubelet token");
        let token =
            tokio::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")
                .await?;
        let url = format!(
            "{}://{service}.{namespace}.svc:{port}/ingest{}",
            if ca_cert_file.is_some() {
                "https"
            } else {
                "http"
            },
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
            .header(
                "X-Scrape-Time-Ms",
                scrape_time_ms_header_value(scrape_duration),
            )
            .header("X-Agent-Version", env!("CARGO_PKG_VERSION"))
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

fn scrape_time_ms_header_value(duration: Duration) -> String {
    duration.as_millis().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrape_time_ms_header_value_formats_milliseconds() {
        assert_eq!(
            scrape_time_ms_header_value(Duration::from_millis(1500)),
            "1500"
        );
    }

    #[test]
    fn scrape_time_ms_header_value_handles_zero() {
        assert_eq!(scrape_time_ms_header_value(Duration::ZERO), "0");
    }

    #[test]
    fn scrape_time_ms_header_value_truncates_sub_millisecond() {
        assert_eq!(scrape_time_ms_header_value(Duration::from_micros(500)), "0");
    }
}
