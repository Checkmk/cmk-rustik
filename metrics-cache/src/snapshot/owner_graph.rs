use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::ResourceExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ingest::reflectors::FrozenStores;
use crate::snapshot::Uid;

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
    /// [`crate::snapshot::Snapshot::new()`].
    pub fn from_frozen_stores(stores: &FrozenStores) -> Self {
        let owner_ref_by_uid =
            Self::map_object_uids_to_owner_ref(&stores.pods, &stores.replicasets, &stores.jobs);
        let pods_by_controller = Self::get_pods_by_controller(&stores.pods, &owner_ref_by_uid);
        OwnerGraph {
            owner_ref_by_uid,
            pods_by_controller,
        }
    }

    /// Create a map, uid-to-OwnerReference, of objects monitored in the
    /// [`crate::ingest::reflectors::Stores`]. We take the first owner which
    /// reports being a controller.
    ///
    /// Theory says there should only be at most one per object anyway.
    /// If one is found, the object's [`Uid`] is used as the key in the map
    /// and the [`OwnerReference`] is taken as the value.
    fn map_object_uids_to_owner_ref(
        pods: &[Arc<Pod>],
        replicasets: &[Arc<ReplicaSet>],
        jobs: &[Arc<Job>],
    ) -> HashMap<Uid, OwnerReference> {
        let mut map = HashMap::new();
        for pod in pods {
            if let Some(owner_controller) = pod
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = pod.metadata.uid.clone()
            {
                map.insert(Uid(uid.into()), owner_controller.to_owned());
            }
        }
        for rs in replicasets {
            if let Some(owner_controller) = rs
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = rs.metadata.uid.clone()
            {
                map.insert(Uid(uid.into()), owner_controller.to_owned());
            }
        }
        for job in jobs {
            if let Some(owner_controller) = job
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = job.metadata.uid.clone()
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
        pods: &[Arc<Pod>],
        owner_ref_by_uid: &HashMap<Uid, OwnerReference>,
    ) -> HashMap<Uid, Vec<Arc<Pod>>> {
        let mut out: HashMap<Uid, Vec<Arc<Pod>>> = HashMap::new();
        for pod in pods {
            let Some(pod_uid) = pod.metadata.uid.as_deref() else {
                continue;
            };
            let chain = Self::walk_up_in(owner_ref_by_uid, pod_uid);
            for parent in chain {
                out.entry(parent.uid.as_str().into())
                    .or_default()
                    .push(pod.clone());
            }
        }
        out
    }

    /// Get all pods that are controlled by the controller at the given
    /// `controller_uid`.
    pub fn pods_by_controller(&self, controller_uid: &str) -> &[Arc<Pod>] {
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::*;

    #[test]
    fn walk_up() {
        let graph = owner_graph(&[
            ("pod-uid", owner_ref("ReplicaSet", "rs", "rs-uid")),
            ("rs-uid", owner_ref("Deployment", "deploy", "deploy-uid")),
        ]);

        // Nearest first: ReplicaSet then Deployment
        let chain = graph.walk_up("pod-uid");
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].uid, "rs-uid");
        assert_eq!(chain[1].uid, "deploy-uid");

        // Starting mid chain
        let chain = graph.walk_up("rs-uid");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].uid, "deploy-uid");

        // Unknown start should not panic
        assert!(graph.walk_up("definitely-does-not-exist").is_empty());
    }

    #[test]
    fn walk_up_no_infinite_cycles() {
        let graph = owner_graph(&[
            ("a", owner_ref("SomeCrd", "b", "b")),
            ("b", owner_ref("SomeCrd", "a", "a")),
        ]);
        let chain = graph.walk_up("a");
        // We start at a, go to b, see a again and stop.
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].uid, "b");
        assert_eq!(chain[1].uid, "a");
    }

    /// Objects get mapped by their uid correctly to their owner reference and
    /// are dropped from the map if they have no owner reference that is marked
    /// as a controller.
    #[test]
    fn map_object_uids_to_owner_ref() {
        let pod1 = pod_owned_by("pod1", "pod1-uid", owner_ref("ReplicaSet", "rs", "rs-uid"));
        let rs = replicaset_owned_by("rs", "rs-uid", owner_ref("Deployment", "dep", "dep-uid"));
        let job = job_owned_by("job1", "job1-uid", owner_ref("CronJob", "cj", "cj-uid"));

        // Non-controller owner references are skipped
        let mut non_controller = owner_ref("ReplicaSet", "rs", "rs-uid");
        non_controller.controller = Some(false);
        let pod_owned_by_non_controller = pod_owned_by("pod-2", "pod2-uid", non_controller);

        let map = OwnerGraph::map_object_uids_to_owner_ref(
            &[pod1.into(), pod_owned_by_non_controller.into()],
            &[rs.into()],
            &[job.into()],
        );

        assert_eq!(map.len(), 3); // the pod with no controller is dropped
        assert_eq!(map["pod1-uid"].uid, "rs-uid");
        assert_eq!(map["rs-uid"].uid, "dep-uid");
        assert_eq!(map["job1-uid"].uid, "cj-uid");
    }

    #[test]
    fn get_pods_by_controller_transitivity() {
        let pods = [
            pod_owned_by("pod1", "pod1-uid", owner_ref("ReplicaSet", "rs", "rs-uid")).into(),
            pod_owned_by("pod2", "pod2-uid", owner_ref("ReplicaSet", "rs", "rs-uid")).into(),
        ];
        let replicasets =
            [
                replicaset_owned_by("rs", "rs-uid", owner_ref("Deployment", "dep", "dep-uid"))
                    .into(),
            ];
        let jobs = [job_owned_by("job1", "job1-uid", owner_ref("CronJob", "cj", "cj-uid")).into()];
        let owner_ref_by_uid = OwnerGraph::map_object_uids_to_owner_ref(&pods, &replicasets, &jobs);
        let pods_owned_by_rs = OwnerGraph::get_pods_by_controller(&pods, &owner_ref_by_uid);

        assert_eq!(pods_owned_by_rs.len(), 2); // two controllers: rs-uid and dep-uid
        assert_eq!(pods_owned_by_rs["dep-uid"].len(), 2); // both pods show up
        assert_eq!(pods_owned_by_rs["rs-uid"].len(), 2); // both pods show up
    }

    #[test]
    fn pods_by_controller() {
        let graph = OwnerGraph {
            owner_ref_by_uid: HashMap::new(),
            pods_by_controller: HashMap::from([("rs-uid".into(), vec![pod("p1", None).into()])]),
        };

        // Known controller: returns its owned pods.
        assert_eq!(graph.pods_by_controller("rs-uid").len(), 1);

        // Unknown controller: empty slice, no panic.
        assert!(graph.pods_by_controller("Does-Not-Exist").is_empty());
    }
}
