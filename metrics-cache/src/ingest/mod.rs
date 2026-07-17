use std::time::Instant;

pub mod kubelet_stats;
pub mod reflectors;

#[derive(Clone, Debug)]
pub struct MetricsFetcherIngestion<T> {
    pub received_at: Instant,
    pub payload: T,
}
