use k8s_openapi::api::core::v1::PersistentVolumeClaim;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ingest::reflectors::FrozenStores;

#[derive(Debug)]
pub struct Indexes {
    /// Persistent Volume Claims, indexed by `pvc[namespace][name]`.
    ///
    /// Pods reference a PVC by name (and only in the same namespace).
    pub pvcs: HashMap<String, HashMap<String, Arc<PersistentVolumeClaim>>>,
}

impl Indexes {
    pub fn from_frozen_stores(stores: &FrozenStores) -> Self {
        Self {
            pvcs: Self::pvcs_by_ns_and_name(stores),
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
}
