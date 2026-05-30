use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet};
use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::ResourceExt;
use std::collections::HashMap;
use std::sync::Arc;

use crate::Stores;
use crate::kube::Uid;

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
    pub pods: Vec<Arc<Pod>>,
    pub nodes: Vec<Arc<Node>>,
    pub deployments: Vec<Arc<Deployment>>,
    pub daemonsets: Vec<Arc<DaemonSet>>,
    pub namespaces: Vec<Arc<Namespace>>,
    pub replicasets: Vec<Arc<ReplicaSet>>,
}

impl Snapshot {
    /// Create a snapshot from the current state of all the monitored
    /// [`Store`]s.
    pub fn new(stores: Stores) -> Self {
        Snapshot {
            pods: stores.pods.state(),
            nodes: stores.nodes.state(),
            deployments: stores.deployments.state(),
            daemonsets: stores.daemonsets.state(),
            namespaces: stores.namespaces.state(),
            replicasets: stores.replicasets.state(),
        }
    }

    // TODO: This doesn't need to be public, it just makes debugging easier for
    // now.
    /// Create a map, uid-to-OwnerReference, of objects monitored in the
    /// [`Stores`]. We take the first owner which reports being a controller.
    /// Theory says there should only be at most one per object anyway.
    /// If one is found, the object's [`Uid`] is used as the key in the map
    /// and the [`OwnerReference`'] is taken as the value.
    ///
    /// Not everything in the [`Stores`] is iterated; only things actually
    /// likely to have a controller owner (e.g. Pods, RepliaSets, ...).
    pub fn map_object_uids_to_owner_ref(&self) -> HashMap<Uid, OwnerReference> {
        let mut map = HashMap::new();
        for pod in &self.pods {
            if let Some(owner_controller) = pod
                .owner_references()
                .iter()
                .find(|r| r.controller == Some(true))
                && let Some(uid) = pod.metadata.uid.clone()
            {
                map.insert(Uid(uid), owner_controller.to_owned());
            }
        }
        for rs in &self.replicasets {
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
}
