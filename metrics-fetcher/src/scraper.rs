use reqwest::Client;
use tokio::time;
use tokio::time::Duration;
use tracing::warn;

use crate::error::Result;
use crate::payload::Payload;

pub(crate) trait Scraper {
    async fn scrape(&self) -> Result<Payload>;

    fn relay_client(&self) -> Client;

    async fn loop_push_scrape(self)
    where
        Self: Sized,
    {
        let mut interval = time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            match self.scrape().await {
                Ok(payload) => {
                    let _ = payload.push_to_metrics_cache(self.relay_client()).await;
                }
                Err(e) => {
                    warn!(%e, "scrape failed");
                }
            }
        }
    }
}
