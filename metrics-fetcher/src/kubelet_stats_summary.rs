use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, trace};

use crate::cli_args::CliArgs;
use crate::error::{Error, Result};
use crate::kubelet;
use crate::payload::Payload;
use crate::scraper::Scraper;

pub(crate) struct KubeletStatsSummaryScraper {
    scrape_client: Client,
    relay_client: Client,
    args: Arc<CliArgs>,
}

impl KubeletStatsSummaryScraper {
    pub(crate) fn new(
        args: Arc<CliArgs>,
        metrics_cache_client: Client,
    ) -> KubeletStatsSummaryScraper {
        let scrape_client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("Could not build scrape client for kubelet stats summary");
        KubeletStatsSummaryScraper {
            scrape_client,
            relay_client: metrics_cache_client,
            args,
        }
    }
}

impl Scraper for KubeletStatsSummaryScraper {
    fn poll_interval(&self) -> Duration {
        self.args.kubelet_stats_poll_interval
    }

    fn relay_client(&self) -> Client {
        self.relay_client.clone()
    }

    fn args(&self) -> Arc<CliArgs> {
        self.args.clone()
    }

    /// Query the Kubelet /stats/summary response and return the raw JSON payload.
    ///
    /// We do not parse the payload or perform any calculations on it here, leaving
    /// these tasks for metrics-cache to do.
    async fn scrape(&self) -> Result<Payload> {
        trace!("reading kubelet token");
        let token = std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")?;
        let node_ip = match std::env::var("NODE_IP") {
            Ok(val) => val,
            Err(err) => {
                error!("NODE_IP not set, cannot scrape kubelet");
                return Err(Error::EnvVar {
                    name: "NODE_IP".to_string(),
                    source: err,
                });
            }
        };
        debug!("fetching Kubelet /stats/summary");
        let response = self
            .scrape_client
            .get(kubelet::url(&node_ip, "/stats/summary")?)
            .bearer_auth(token.trim())
            .send()
            .await?;
        debug!(status = %response.status(), "scrape complete");
        Ok(Payload::KubeletStatsSummary(response.bytes().await?))
    }
}
