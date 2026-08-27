use std::time::Instant;

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
#[derive(Clone, Debug)]
pub struct MetricsFetcherIngestion<T> {
    pub received_at: Instant,
    pub payload: T,
}
