use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::ResourceExt;
use std::collections::{HashMap, HashSet};

use crate::kube::Uid;
use crate::{FrozenStores, Stores};

/// Represents a single, static snapshot of the state of the cluster as it
/// pertains to Checkmk monitoring.
///
/// Notably: At construction time, a `Snapshot` is fed from the [`Store`]s
/// stored in our [`Stores`] (which lives in the Axum state via [`AppState`])
/// and the stores are iterated through once to construct the snapshot.
///
/// This means if the store changes (because a new update comes in from the
/// Kubernetes watch API), we don't have to worry about our snapshot state
/// becoming out of date.
///
/// We also create and store the owner graph as part of the snapshot.
#[derive(Debug)]
pub struct Snapshot {
    pub stores: FrozenStores,
    pub owner_graph: OwnerGraph,
}

impl Snapshot {
    /// Create a snapshot from the current state of all the monitored
    /// [`Store`]s.
    pub fn new(stores: Stores) -> Self {
        let stores = stores.freeze();
        let owner_graph = OwnerGraph::from_frozen_stores(&stores);
        Snapshot {
            stores,
            owner_graph,
        }
    }
}

/// An [`OwnerGraph`] is a structure which allows for lookups of what resource
/// owns another resource, if any. For example, a [`Pod`] might be owned by a
/// [`ReplicaSet`] which might be owned by a [`Deployment`].
///
/// `OwnerGraph` owns the structures to perform such lookups and exposes
/// functions to make use of said structures.
#[derive(Debug)]
pub struct OwnerGraph {
    pub owner_ref_by_uid: HashMap<Uid, OwnerReference>,
    //pub pods_by_controller: HashMap<Uid, Vec<Uid>>,
}

impl OwnerGraph {
    pub fn from_frozen_stores(stores: &FrozenStores) -> Self {
        OwnerGraph {
            owner_ref_by_uid: Self::map_object_uids_to_owner_ref(stores),
        }
    }

    /// Create a map, uid-to-OwnerReference, of objects monitored in the
    /// [`Stores`]. We take the first owner which reports being a controller.
    /// Theory says there should only be at most one per object anyway.
    /// If one is found, the object's [`Uid`] is used as the key in the map
    /// and the [`OwnerReference`'] is taken as the value.
    ///
    /// Not everything in the [`Stores`] is iterated; only things actually
    /// likely to have a controller owner (e.g. Pods, RepliaSets, ...).
    fn map_object_uids_to_owner_ref(stores: &FrozenStores) -> HashMap<Uid, OwnerReference> {
        let mut map = HashMap::new();
        for pod in &stores.pods {
            if let Some(owner_controller) = pod
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = pod.metadata.uid.clone()
            {
                map.insert(Uid(uid), owner_controller.to_owned());
            }
        }
        for rs in &stores.replicasets {
            if let Some(owner_controller) = rs
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = rs.metadata.uid.clone()
            {
                map.insert(Uid(uid), owner_controller.to_owned());
            }
        }
        map
    }

    /// Walk the ownership chain from `start`, returning the
    /// [`OwnerReference`]s from the direct controller up to the root.
    ///
    /// Looks up the controller of `start`, then that controller's controller
    /// and so on, until it reaches an object with no controlling owner (and
    /// thus not in `owner_ref_by_uid`).
    ///
    /// The result is ordered nearest-first (so: a Pod owned by a RepliaSet
    /// owned by a Deployment will yield a vector containing first the
    /// ReplicaSet and then the Deployment).
    ///
    /// The chain may be incomplete: if an owner hasn't been observed by the
    /// watch yet, the walk simply stops there (eventual consistency, not an
    /// error). Kubernetes should never produce a cycle, but this is
    /// nevertheless guarded against; the walk always terminates.
    pub fn walk_up(&self, start: &str) -> Vec<&OwnerReference> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = self.owner_ref_by_uid.get(start);
        while let Some(owner_ref) = cur {
            if !seen.insert(owner_ref.uid.as_str()) {
                break;
            }
            chain.push(owner_ref);
            cur = self.owner_ref_by_uid.get(owner_ref.uid.as_str());
        }
        chain
    }
}
