pub mod indexes;
pub mod kubelet_health;
pub mod metric_tables;
pub mod owner_graph;
pub mod self_health;
pub mod system_agent;

use crate::ingest::kubelet_health::KubeletHealth;
use crate::ingest::kubelet_stats::StatsSummary;
use crate::ingest::reflectors::{FrozenStores, Stores};
use crate::ingest::{MetricsFetcherIngestion, SystemAgentOutput};
use crate::snapshot::indexes::Indexes;
use crate::snapshot::kubelet_health::KubeletHealths;
use crate::snapshot::metric_tables::MetricTables;
use crate::snapshot::owner_graph::OwnerGraph;
use crate::snapshot::self_health::SelfHealth;
use crate::snapshot::system_agent::SystemAgentOutputs;
use moka::future::Cache;
use std::borrow::Borrow;
use std::sync::Arc;
use std::time::Instant;

/// Represents a single, static snapshot of the state of the cluster as it
/// pertains to Checkmk monitoring.
///
/// Notably: At construction time, a `Snapshot` is fed from the
/// [`kube::runtime::reflector::Store`]s stored in our [`Stores`] (which lives
/// in the Axum state via [`crate::state::AppState`]) and the stores are
/// iterated through once to construct the snapshot.
///
/// This means if the store changes (because a new update comes in from the
/// Kubernetes watch API), we don't have to worry about our snapshot state
/// becoming inconsistent, the new state is simply ignored in this snapshot.
///
/// We also create and store the [`OwnerGraph`] as part of the snapshot and
/// indexes useful for looking up resources by particular keys.
#[derive(Debug)]
pub struct Snapshot {
    pub instant: Instant,
    pub stores: FrozenStores,
    pub owner_graph: OwnerGraph,
    pub metrics: MetricTables,
    pub indexes: Indexes,
    pub self_health: SelfHealth,
    pub(crate) kubelet_health: KubeletHealths,
    pub(crate) system_agents: SystemAgentOutputs,
}

impl Snapshot {
    /// Create a snapshot from the current state of all the monitored
    /// [`kube::runtime::reflector::Store`]s and all stat summaries scraped from the Kubelet.
    pub fn new(
        stores: Stores,
        kubelet_stats_summary_cache: Cache<String, Arc<MetricsFetcherIngestion<StatsSummary>>>,
        kubelet_health_cache: Cache<String, Arc<MetricsFetcherIngestion<KubeletHealth>>>,
        system_agent_cache: Cache<String, Arc<MetricsFetcherIngestion<SystemAgentOutput>>>,
    ) -> Self {
        let instant = Instant::now();
        let reflector_healths = stores.freeze_healths();
        let stores = stores.freeze();
        let owner_graph = OwnerGraph::from_frozen_stores(&stores);
        let metrics = MetricTables::from_cache(&kubelet_stats_summary_cache);
        let indexes = Indexes::from_frozen_stores(&stores);
        let self_health = SelfHealth::new(
            instant,
            &stores.nodes,
            reflector_healths,
            &kubelet_stats_summary_cache,
        );
        let kubelet_health = KubeletHealths::from_cache(&kubelet_health_cache);
        let system_agents = SystemAgentOutputs::from_cache(&system_agent_cache);
        Snapshot {
            instant,
            stores,
            owner_graph,
            metrics,
            indexes,
            self_health,
            kubelet_health,
            system_agents,
        }
    }
}

/// The unique ID of a given Kubernetes object.
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct Uid(pub Arc<str>);

impl Borrow<str> for Uid {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Uid {
    #[inline]
    fn from(s: &str) -> Uid {
        Uid(s.into())
    }
}

impl From<String> for Uid {
    #[inline]
    fn from(s: String) -> Uid {
        Uid(s.into())
    }
}
