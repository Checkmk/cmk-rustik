use k8s_openapi::api::core::v1::Pod;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::section::Section;
use crate::section::common::parse_quantity;
use crate::snapshot::indexes::Indexes;
use crate::snapshot::metric_tables::MetricTables;

#[derive(Debug, Serialize)]
enum Phase {
    #[serde(rename = "Pending")]
    Pending,
    #[serde(rename = "Bound")]
    Bound,
    #[serde(rename = "Lost")]
    Lost,
}

impl Phase {
    pub fn new(s: &str) -> Option<Self> {
        match s {
            "Pending" => Some(Self::Pending),
            "Bound" => Some(Self::Bound),
            "Lost" => Some(Self::Lost),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct StorageRequirement {
    storage: u64,
}

#[derive(Debug, Serialize)]
struct Status<'a> {
    phase: Option<Phase>,
    capacity: Option<StorageRequirement>,
    current_volume_attributes_class_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct Metadata<'a> {
    name: &'a str,
    namespace: &'a str,
}

#[derive(Debug, Serialize)]
struct Claim<'a> {
    metadata: Metadata<'a>,
    status: Status<'a>,
    volume_name: Option<&'a str>,
}

/// PVC overview. (`kube_pvc_v1`)
///
/// At a high level, this section represents the join of two kinds of data from
/// the same source: The Kubernetes API.
///
/// A Pod requests one or more volumes (and of those volumes, we only concern
/// ourselves with PVCs). It references these PVCs by their name (which must be
/// Namespace-unique and the Namespace must be the same as the Pod itself).
///
/// Once we know the names of all the PVCs (i.e., once we have the
/// "claim_names"), we can then look up each of the PVCs and the necessary
/// information about it.
///
/// We _could_ do that lookup on the fly, but since multiple pods likely exist
/// in the same namespace, we avoid needing to loop over the PVCs multiple times
/// by instead caching a mapping in the [`crate::snapshot::Snapshot`]. In
/// particular, we build a `HashMap` that is indexed by the PVC's name, which is
/// what a pod's claim name resolves to, like this:
/// `map[namespace][claim_name] -> PVC`.
///
/// Then in the implementation below, we simply take `namespace` and the list
/// of claim names we care about (determined by [`Self::pod_pvc_claim_names()`]
/// and [`Self::workload_pvc_claim_names()`] at the section-emit call site) and
/// return information about each PVC keyed on its claim name.
#[derive(Debug, Default, Serialize)]
pub(crate) struct KubePvcV1<'a> {
    claims: BTreeMap<String, Claim<'a>>,
}

impl<'a> KubePvcV1<'a> {
    /// Given a snapshot, a namespace in which the claims live, and the claim
    /// names for which we want PVC information, generate the PVC information
    /// if able.
    ///
    /// In several cases, we might not "be able" to generate PVC information.
    ///
    /// For example, if a claim name is provided which does not exist in
    /// `indexes` (and therefore was not known by the Kubernetes API at the
    /// time of snapshot capture), that claim name is skipped.
    ///
    /// If we get to the end and would not produce information for _any_ claim
    /// name, we return `None` so that the section-emit call site can omit the
    /// section entirely.
    pub fn from_claim_names(
        indexes: &'a Indexes,
        namespace: &'a str,
        claim_names: impl IntoIterator<Item = &'a str>,
    ) -> Option<KubePvcV1<'a>> {
        let mut out = Self::default();
        for claim_name in claim_names {
            let Some(pvc) = indexes.pvc(namespace, claim_name) else {
                continue;
            };
            let capacity = pvc
                .status
                .as_ref()
                .and_then(|s| s.capacity.as_ref())
                .and_then(|b| b.get("storage"))
                .and_then(|q| parse_quantity(&q.0))
                .map(|v| StorageRequirement { storage: v as u64 });
            let claim = Claim {
                metadata: Metadata {
                    name: claim_name,
                    namespace,
                },
                status: Status {
                    phase: pvc
                        .status
                        .as_ref()
                        .and_then(|s| s.phase.as_deref())
                        .and_then(Phase::new),
                    capacity,
                    current_volume_attributes_class_name: pvc
                        .status
                        .as_ref()
                        .and_then(|s| s.current_volume_attributes_class_name.as_deref()),
                },
                volume_name: pvc.spec.as_ref().and_then(|s| s.volume_name.as_deref()),
            };
            out.claims.insert(claim_name.to_string(), claim);
        }

        if out.claims.is_empty() {
            return None;
        }

        Some(out)
    }

    /// Given an [`Pod`] reference, get all of the claim names it
    /// references in its specification.
    ///
    /// Only PVCs are kept, any other kind of volume is discarded.
    pub fn pod_pvc_claim_names(pod: &Pod) -> impl Iterator<Item = &str> {
        pod.spec
            .as_ref()
            .and_then(|p| p.volumes.as_ref())
            .into_iter()
            .flatten()
            .filter_map(|v| v.persistent_volume_claim.as_ref())
            .map(|s| s.claim_name.as_str())
    }

    /// Given a workload, get all of the claim names it references in its Pods'
    /// specifications.
    ///
    /// Only PVCs are kept, any other kind of volume is discarded.
    ///
    /// This is a convenience wrapper over [`Self::pod_pvc_claim_names()`].
    #[allow(dead_code)]
    pub fn workload_pvc_claim_names(pods: &[Arc<Pod>]) -> HashSet<&str> {
        pods.iter()
            .flat_map(|p| Self::pod_pvc_claim_names(p))
            .collect()
    }
}

impl Section for KubePvcV1<'_> {
    const NAME: &'static str = "kube_pvc_v1";
}

#[derive(Debug, Serialize)]
struct AttachedVolume<'a> {
    capacity: f64,
    free: f64,
    persistent_volume_claim: &'a str,
    namespace: &'a str,
}

/// Live usage data for PVCs. (`kube_pvc_volumes_v1`)
#[derive(Debug, Default, Serialize)]
pub(crate) struct KubePvcVolumesV1<'a> {
    volumes: BTreeMap<String, AttachedVolume<'a>>,
}

impl<'a> KubePvcVolumesV1<'a> {
    /// Uses data from the given [`MetricTables`] to provide real-time usage
    /// data for PVC volumes, in contrast to [`KubePvcV1::from_claim_names()`]
    /// which uses only API data and does not provide usage data.
    ///
    /// Returns None if no claim has volume stats.
    pub fn from_claim_names(
        metric_tables: &'a MetricTables,
        namespace: &'a str,
        claim_names: impl IntoIterator<Item = &'a str>,
    ) -> Option<KubePvcVolumesV1<'a>> {
        let mut out = Self::default();
        for claim_name in claim_names {
            let Some(sample) = metric_tables.pvc_volume(namespace, claim_name) else {
                continue;
            };
            let volume = AttachedVolume {
                capacity: sample.capacity_bytes as f64,
                free: sample.available_bytes as f64,
                persistent_volume_claim: claim_name,
                namespace,
            };
            out.volumes.insert(claim_name.to_string(), volume);
        }

        if out.volumes.is_empty() {
            return None;
        }

        Some(out)
    }
}

impl Section for KubePvcVolumesV1<'_> {
    const NAME: &'static str = "kube_pvc_volumes_v1";
}

#[derive(Debug, Serialize)]
struct PersistentVolumeSpec<'a> {
    access_modes: &'a [String],
    storage_class_name: Option<&'a str>,
    volume_mode: &'a str,
}

#[derive(Debug, Serialize)]
struct PersistentVolume<'a> {
    name: &'a str,
    spec: PersistentVolumeSpec<'a>,
}

/// Persistent volumes. (`kube_pvc_pvs_v1`)
///
/// Mainly used in the details section of the PVC services.
#[derive(Debug, Default, Serialize)]
pub(crate) struct KubePvcPvsV1<'a> {
    volumes: BTreeMap<String, PersistentVolume<'a>>,
}

impl<'a> KubePvcPvsV1<'a> {
    /// Uses the PV index to collect information about the PVs that back the
    /// PVCs provided by the claim names given.
    ///
    /// Returns None if no claims have a backing PV.
    pub fn from_claim_names(
        indexes: &'a Indexes,
        namespace: &'a str,
        claim_names: impl IntoIterator<Item = &'a str>,
    ) -> Option<KubePvcPvsV1<'a>> {
        let mut out = Self::default();
        for claim_name in claim_names {
            let Some(pvc) = indexes.pvc(namespace, claim_name) else {
                continue;
            };

            let Some(volume_name) = pvc.spec.as_ref().and_then(|s| s.volume_name.as_deref()) else {
                continue;
            };

            let Some(pv) = indexes.pv(volume_name) else {
                continue;
            };

            let access_modes: &'a [String] = pv
                .spec
                .as_ref()
                .and_then(|spec| spec.access_modes.as_deref())
                .unwrap_or(&[]);

            let storage_class_name = pv
                .spec
                .as_ref()
                .and_then(|s| s.storage_class_name.as_deref());

            let volume_mode = pv
                .spec
                .as_ref()
                .and_then(|s| s.volume_mode.as_deref())
                .unwrap_or("Filesystem");

            let pv_spec = PersistentVolumeSpec {
                access_modes,
                storage_class_name,
                volume_mode,
            };

            let persistent_volume = PersistentVolume {
                name: volume_name,
                spec: pv_spec,
            };

            out.volumes
                .insert(volume_name.to_string(), persistent_volume);
        }

        if out.volumes.is_empty() {
            return None;
        }

        Some(out)
    }
}

impl Section for KubePvcPvsV1<'_> {
    const NAME: &'static str = "kube_pvc_pvs_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    use crate::test_support::*;

    fn pvc_indexes() -> Indexes {
        let pvc1 = pvc("pvc-1");
        let pvc2 = pvc("pvc-2");
        let mut indexes = Indexes::default();
        for pvc in [pvc1, pvc2] {
            indexes
                .pvcs
                .entry(pvc.metadata.namespace.clone().unwrap())
                .or_default()
                .insert(pvc.metadata.name.clone().unwrap(), pvc.into());
        }
        indexes
    }

    #[test]
    fn kube_pvc_v1() {
        let indexes = pvc_indexes();

        // All claim names match, section render
        insta::assert_json_snapshot!(KubePvcV1::from_claim_names(
            &indexes,
            "really-cool-namespace",
            ["pvc-1", "pvc-2"]
        ));

        // If no claim names match, do not produce a section at all
        assert_matches!(
            KubePvcV1::from_claim_names(&indexes, "really-cool-namespace", ["bad", "worse"]),
            None
        );

        // But if one does, then do produce a section
        assert_eq!(
            KubePvcV1::from_claim_names(&indexes, "really-cool-namespace", ["bad", "pvc-1"])
                .unwrap()
                .claims
                .len(),
            1
        );

        // Order of claim names does not matter
        assert_eq!(
            KubePvcV1::from_claim_names(&indexes, "really-cool-namespace", ["pvc-1", "bad"])
                .unwrap()
                .claims
                .len(),
            1
        );

        // If everything is in another namespace, do not produce a section
        assert_matches!(
            KubePvcV1::from_claim_names(&indexes, "ANOTHER-namespace", ["pvc-1", "pvc-2"]),
            None
        );
    }
}
