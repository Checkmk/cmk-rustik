use reqwest::Client;
use tracing::{debug, trace};

use crate::error::Result;
use crate::payload::Payload;
use crate::scraper::Scraper;

pub(crate) struct KubeletStatsSummaryScraper {
    scrape_client: Client,
    relay_client: Client,
}

impl KubeletStatsSummaryScraper {
    pub(crate) fn new() -> Self {
        let scrape_client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("Could not build scrape client for kubelet stats summary");
        Self {
            scrape_client,
            relay_client: Client::new(),
        }
    }
}

impl Scraper for KubeletStatsSummaryScraper {
    fn relay_client(&self) -> Client {
        self.relay_client.clone()
    }

    /// Query the Kubelet /stats/summary response and return the raw JSON payload.
    ///
    /// We do not parse the payload or perform any calculations on it here, leaving
    /// these tasks for metrics-cache to do.
    async fn scrape(&self) -> Result<Payload> {
        trace!("reading kubelet token");
        let token = std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")?;
        debug!("fetching Kubelet /stats/summary");
        let response = self
            .scrape_client
            .get("https://127.0.0.1:10250/stats/summary")
            .bearer_auth(token.trim())
            .send()
            .await?;
        debug!(status = %response.status(), "scrape complete");
        Ok(Payload::KubeletStatsSummary(response.bytes().await?))
    }
}
