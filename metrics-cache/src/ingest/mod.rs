use axum::body::Bytes;
use std::time::Instant;

pub mod kubelet_health;
pub mod kubelet_stats;
pub mod reflectors;

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

/// Raw output from a machine-level agent (currently only Linux's
/// `check_mk_agent`, but named generically since a Windows agent could push
/// here too some day). A distinct type rather than a bare [`Bytes`], so the
/// cache's shape stays unambiguous if another `Bytes`-based payload is ever
/// added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemAgentOutput(pub Bytes);
