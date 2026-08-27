use axum::http::header::HeaderMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

pub mod api_health;
pub mod kubelet_health;
pub mod kubelet_stats;
pub mod reflectors;
pub mod system_agent;

/// A payload received from `metrics-fetcher`, along with the [`Instant`] it
/// was received. This is stored in moka caches in [`crate::state::AppState`].
///
/// The timestamp is used for self-health monitoring, so that we can report
/// how long it's been since we last heard from a node.
#[derive(Debug)]
pub struct MetricsFetcherIngestion<T> {
    pub received_at: Instant,
    pub metadata: MetricsFetcherMetadata,
    pub payload: T,
}

/// Metadata about a payload received from `metrics-fetcher`, such as
/// performance information (how long a scrape took) and the version of rustik
/// that `metrics-fetcher` instance came from.
#[derive(Debug, Default)]
pub struct MetricsFetcherMetadata {
    pub scrape_time: Option<Duration>,
    pub version: Option<String>,
}

impl From<&HeaderMap> for MetricsFetcherMetadata {
    fn from(headers: &HeaderMap) -> Self {
        Self {
            scrape_time: headers
                .get("X-Scrape-Time-Ms")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| u64::from_str(v).ok())
                .map(Duration::from_millis),
            version: headers
                .get("X-Agent-Version")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
        }
    }
}
