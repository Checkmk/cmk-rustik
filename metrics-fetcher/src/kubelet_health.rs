use bytes::Bytes;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::cli_args::CliArgs;
use crate::error::{Error, Result};
use crate::payload::Payload;
use crate::scraper::Scraper;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum KubeletHealth {
    Response { status_code: u16, response: String },
    ConnectionError { message: String },
}

pub(crate) struct KubeletHealthScraper {
    scrape_client: Client,
    relay_client: Client,
    args: Arc<CliArgs>,
}

impl KubeletHealthScraper {
    pub(crate) fn new(args: Arc<CliArgs>, metrics_cache_client: Client) -> KubeletHealthScraper {
        let scrape_client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("Could not build scrape client for kubelet health");
        KubeletHealthScraper {
            scrape_client,
            relay_client: metrics_cache_client,
            args,
        }
    }
}

impl Scraper for KubeletHealthScraper {
    fn relay_client(&self) -> Client {
        self.relay_client.clone()
    }

    fn args(&self) -> Arc<CliArgs> {
        self.args.clone()
    }

    async fn scrape(&self) -> Result<Payload> {
        let node_ip = std::env::var("NODE_IP").map_err(|e| Error::EnvVar {
            name: "NODE_IP".to_string(),
            source: e,
        })?;
        let node_name = std::env::var("NODE_NAME").map_err(|e| Error::EnvVar {
            name: "NODE_NAME".to_string(),
            source: e,
        })?;
        let token =
            tokio::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")
                .await?;

        let url = format!("https://{node_ip}:10250/healthz");
        let response = self
            .scrape_client
            .get(&url)
            .bearer_auth(token.trim())
            .send()
            .await;

        let health = match response {
            Ok(response) => {
                let status_code = response.status();
                if status_code == StatusCode::OK {
                    debug!(status = %status_code, url, "kubelet healthz scrape complete");
                } else {
                    warn!(status = %status_code, url, "kubelet healthz scrape returned non-OK status");
                }
                let response_body = response.text().await?;
                KubeletHealth::Response {
                    status_code: status_code.as_u16(),
                    response: response_body,
                }
            }
            Err(e) => KubeletHealth::ConnectionError {
                message: e.to_string(),
            },
        };

        Ok(Payload::KubeletHealth {
            node_name,
            body: Bytes::from(serde_json::to_vec(&health)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kubelet_health_response_wire_shape() {
        let health = KubeletHealth::Response {
            status_code: 200,
            response: "ok".to_string(),
        };
        assert_eq!(
            serde_json::to_string(&health).expect("KubeletHealth always serializes"),
            r#"{"status_code":200,"response":"ok"}"#
        );
    }

    #[test]
    fn kubelet_health_connection_error_wire_shape() {
        let health = KubeletHealth::ConnectionError {
            message: "connection refused".to_string(),
        };
        assert_eq!(
            serde_json::to_string(&health).expect("KubeletHealth always serializes"),
            r#"{"message":"connection refused"}"#
        );
    }
}
