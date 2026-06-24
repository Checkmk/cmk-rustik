use k8s_openapi::api::core::v1::{PersistentVolumeClaim, Pod};
use std::collections::HashMap;
use std::sync::Arc;

use crate::ingest::reflectors::FrozenStores;

#[derive(Debug)]
pub struct Indexes {
    /// Map namespace names to all the pods within that namespace.
    pub pods_by_namespace: HashMap<String, Vec<Arc<Pod>>>,

    /// Persistent Volume Claims, indexed by `pvc = pvcs[namespace][name]`.
    ///
    /// Pods reference a PVC by name (and only in the same namespace).
    pub pvcs: HashMap<String, HashMap<String, Arc<PersistentVolumeClaim>>>,
}

impl Indexes {
    pub fn from_frozen_stores(stores: &FrozenStores) -> Self {
        Self {
            pvcs: Self::pvcs_by_ns_and_name(stores),
            pods_by_namespace: Self::get_pods_by_namespace(stores),
        }
    }

    /// Iterate the PVCs in the store and map them out to be keyed on their
    /// namespace and name. O(n) over the PVCs in the store.
    fn pvcs_by_ns_and_name(
        stores: &FrozenStores,
    ) -> HashMap<String, HashMap<String, Arc<PersistentVolumeClaim>>> {
        let mut out: HashMap<String, HashMap<String, Arc<PersistentVolumeClaim>>> = HashMap::new();
        for pvc in &stores.persistent_volume_claims {
            let Some(namespace) = pvc.metadata.namespace.clone() else {
                continue;
            };
            let Some(name) = pvc.metadata.name.clone() else {
                continue;
            };
            out.entry(namespace).or_default().insert(name, pvc.clone());
        }
        out
    }

    /// Given a namespace and a PVC name, try to find the PVC. O(1).
    pub fn pvc(&self, namespace: &str, name: &str) -> Option<&PersistentVolumeClaim> {
        self.pvcs.get(namespace)?.get(name).map(Arc::as_ref)
    }

    /// List all the pods in the given namespace
    pub fn pods_by_namespace(&self, namespace: &str) -> &[Arc<Pod>] {
        self.pods_by_namespace
            .get(namespace)
            .map_or(&[], Vec::as_slice)
    }

    /// Compute all the pods associated with each namespace in the snapshot.
    fn get_pods_by_namespace(stores: &FrozenStores) -> HashMap<String, Vec<Arc<Pod>>> {
        let mut out: HashMap<String, Vec<Arc<Pod>>> = HashMap::new();
        for pod in &stores.pods {
            let Some(namespace) = &pod.metadata.namespace else {
                continue;
            };
            if let Some(namespace_vec) = out.get_mut(namespace) {
                namespace_vec.push(pod.clone());
            } else {
                out.insert(namespace.clone(), vec![pod.clone()]);
            }
        }
        out
    }
}
