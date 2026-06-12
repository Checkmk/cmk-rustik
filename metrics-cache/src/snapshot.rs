use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::ResourceExt;
use moka::future::Cache;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ingest::kubelet_stats::StatsSummary;
use crate::kube::Uid;
use crate::{FrozenStores, Stores};

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
/// becoming out of date, the new state is simply ignored in this snapshot.
///
/// We also create and store the [`OwnerGraph`] as part of the snapshot.
#[derive(Debug)]
pub struct Snapshot {
    pub stores: FrozenStores,
    pub owner_graph: OwnerGraph,
    pub metrics: MetricTables,
}

impl Snapshot {
    /// Create a snapshot from the current state of all the monitored
    /// [`kube::runtime::reflector::Store`]s and all stat summaries scraped from the Kubelet.
    pub fn new(
        stores: Stores,
        kubelet_stats_summary_cache: Cache<String, Arc<StatsSummary>>,
    ) -> Self {
        let stores = stores.freeze();
        let owner_graph = OwnerGraph::from_frozen_stores(&stores);
        let metrics = MetricTables::from_cache(kubelet_stats_summary_cache);
        Snapshot {
            stores,
            owner_graph,
            metrics,
        }
    }
}

/// An [`OwnerGraph`] is a structure which allows for lookups of which resource
/// owns another resource, if any. For example, a Pod might be owned by a
/// ReplicaSet which might be owned by a Deployment.
///
/// `OwnerGraph` owns the structures to perform such lookups and exposes
/// functions to make use of said structures.
///
/// When the graph is created, internally two maps are created:
///
/// - The first maps Kubernetes [`Uid`]s of all "controllable" objects that we
///   monitor (e.g. Pods, ReplicaSets, etc.) to [`OwnerReference`]s. This is a
///   **direct, non-recursive** mapping: one object to its immediate owner as
///   reported by Kubernetes. The only requirement is that the owner is listed
///   as a (_the_) controller for the object.
///
/// - The second maps controllers (things that can own other things), by their
///   [`Uid`]s, to a vector containing the pods that they own. This mapping is
///   recursive (represents the transitive closure of all the pods owned by the
///   controller in the key).
#[derive(Debug)]
pub struct OwnerGraph {
    pub owner_ref_by_uid: HashMap<Uid, OwnerReference>,
    pub pods_by_controller: HashMap<Uid, Vec<Arc<Pod>>>,
}

impl OwnerGraph {
    /// Construct an `OwnerGraph` given a [`FrozenStores`] reference.
    ///
    /// Outside of testing, this normally only gets called by
    /// [`Snapshot::new()`].
    pub fn from_frozen_stores(stores: &FrozenStores) -> Self {
        let owner_ref_by_uid = Self::map_object_uids_to_owner_ref(stores);
        let pods_by_controller = Self::get_pods_by_controller(stores, &owner_ref_by_uid);
        OwnerGraph {
            owner_ref_by_uid,
            pods_by_controller,
        }
    }

    /// Create a map, uid-to-OwnerReference, of objects monitored in the
    /// [`Stores`]. We take the first owner which reports being a controller.
    /// Theory says there should only be at most one per object anyway.
    /// If one is found, the object's [`Uid`] is used as the key in the map
    /// and the [`OwnerReference`] is taken as the value.
    ///
    /// Not everything in the [`Stores`] is iterated; only things actually
    /// likely to have a controller owner (e.g. Pods, ReplicaSets, ...).
    fn map_object_uids_to_owner_ref(stores: &FrozenStores) -> HashMap<Uid, OwnerReference> {
        let mut map = HashMap::new();
        for pod in &stores.pods {
            if let Some(owner_controller) = pod
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = pod.metadata.uid.clone()
            {
                map.insert(Uid(uid.into()), owner_controller.to_owned());
            }
        }
        for rs in &stores.replicasets {
            if let Some(owner_controller) = rs
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = rs.metadata.uid.clone()
            {
                map.insert(Uid(uid.into()), owner_controller.to_owned());
            }
        }
        map
    }

    /// Build the inverse index: for each controller UID, the UIDs of all pods
    /// it transitively owns. Each pod is registered under every ancestor in its
    /// chain, so, for example, a Deployment entry includes pods owned via its
    /// ReplicaSets.
    fn get_pods_by_controller(
        stores: &FrozenStores,
        owner_ref_by_uid: &HashMap<Uid, OwnerReference>,
    ) -> HashMap<Uid, Vec<Arc<Pod>>> {
        let mut out: HashMap<Uid, Vec<Arc<Pod>>> = HashMap::new();
        for pod in &stores.pods {
            let Some(pod_uid) = pod.metadata.uid.as_deref() else {
                continue;
            };
            let chain = Self::walk_up_in(owner_ref_by_uid, pod_uid);
            for parent in chain {
                out.entry(Uid(parent.uid.as_str().into()))
                    .or_default()
                    .push(pod.clone());
            }
        }
        out
    }

    /// Get the [`Uid`] of all pods that are controlled by the controller at the
    /// given `controller_uid`.
    pub fn pods_by_controller(&self, controller_uid: &Uid) -> &[Arc<Pod>] {
        self.pods_by_controller
            .get(controller_uid)
            .map_or(&[], Vec::as_slice)
    }

    /// Like [`Self::walk_up()`] but operates on a borrowed map directly so that
    /// it can be called during construction before the graph exists.
    fn walk_up_in<'a>(
        owner_ref_by_uid: &'a HashMap<Uid, OwnerReference>,
        start: &str,
    ) -> Vec<&'a OwnerReference> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = owner_ref_by_uid.get(start);
        while let Some(owner_ref) = cur {
            if !seen.insert(owner_ref.uid.as_str()) {
                break;
            }
            chain.push(owner_ref);
            cur = owner_ref_by_uid.get(owner_ref.uid.as_str());
        }
        chain
    }

    /// Walk the ownership chain from `start`, returning the
    /// [`OwnerReference`]s from the direct controller up to the root.
    ///
    /// Looks up the controller of `start`, then that controller's controller
    /// and so on, until it reaches an object with no controlling owner (and
    /// thus not in `owner_ref_by_uid`).
    ///
    /// The result is ordered nearest-first (so: a Pod owned by a ReplicaSet
    /// owned by a Deployment will yield a vector containing first the
    /// ReplicaSet and then the Deployment).
    ///
    /// The chain may be incomplete: if an owner hasn't been observed by the
    /// watch yet, the walk simply stops there (eventual consistency, not an
    /// error). Kubernetes should never produce a cycle, but this is
    /// nevertheless guarded against; the walk always terminates.
    pub fn walk_up(&self, start: &str) -> Vec<&OwnerReference> {
        Self::walk_up_in(&self.owner_ref_by_uid, start)
    }
}

#[derive(Debug)]
pub struct MetricTables {
    /// Performance samples for containers.
    ///
    /// Indexed by: `sample = samples[namespace][pod][container]`
    pub containers: HashMap<String, HashMap<String, HashMap<String, Sample>>>,
}

impl MetricTables {
    pub fn from_cache(kubelet_stats_summary_cache: Cache<String, Arc<StatsSummary>>) -> Self {
        let mut containers: HashMap<String, HashMap<String, HashMap<String, Sample>>> =
            HashMap::new();

        for (_, stats_summary) in kubelet_stats_summary_cache.iter() {
            for pod in &stats_summary.pods {
                let pod_map = containers
                    .entry(pod.pod_ref.namespace.clone())
                    .or_default()
                    .entry(pod.pod_ref.name.clone())
                    .or_default();
                for container in &pod.containers {
                    let sample = Sample {
                        cpu_usage_nano_cores: container
                            .cpu
                            .as_ref()
                            .and_then(|c| c.usage_nano_cores),
                        memory_working_set_bytes: container
                            .memory
                            .as_ref()
                            .and_then(|m| m.working_set_bytes),
                    };
                    pod_map.insert(container.name.clone(), sample);
                }
            }
        }

        Self { containers }
    }

    pub fn container(&self, namespace: &str, pod: &str, container: &str) -> Option<&Sample> {
        self.containers.get(namespace)?.get(pod)?.get(container)
    }
}

#[derive(Debug)]
pub struct Sample {
    pub cpu_usage_nano_cores: Option<u64>,
    pub memory_working_set_bytes: Option<u64>,
}
