use reqwest::Client;
use std::sync::Arc;
use tokio::time;
use tokio::time::Duration;
use tracing::{error, warn};

use crate::cli_args::CliArgs;
use crate::error::Result;
use crate::payload::Payload;

pub(crate) trait Scraper {
    async fn scrape(&self) -> Result<Payload>;

    fn relay_client(&self) -> Client;

    fn args(&self) -> Arc<CliArgs>;

    async fn loop_push_scrape(self)
    where
        Self: Sized,
    {
        let mut interval = time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            match self.scrape().await {
                Ok(payload) => {
                    let args = self.args();
                    match payload
                        .push_to_metrics_cache(
                            &args.metrics_cache_namespace,
                            &args.metrics_cache_service,
                            args.metrics_cache_port,
                            self.relay_client(),
                        )
                        .await
                    {
                        Ok(_) => {}
                        Err(e) => error!(error = ?e, "failed to push to metrics-cache"),
                    }
                }
                Err(e) => warn!(error = ?e, "scrape failed"),
            }
        }
    }
}
