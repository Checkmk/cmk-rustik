use serde::Serialize;
use std::collections::BTreeMap;

use crate::section::{
    Section,
    common::{Controller, LabelRef},
};

#[derive(Serialize)]
pub(crate) enum QosClass {
    #[serde(rename = "burstable")]
    Burstable,
    #[serde(rename = "besteffort")]
    BestEffort,
    #[serde(rename = "guaranteed")]
    Guaranteed,
}

impl QosClass {
    pub(crate) fn from_str(qos_class: &str) -> Option<Self> {
        match qos_class {
            "Burstable" => Some(QosClass::Burstable),
            "BestEffort" => Some(QosClass::BestEffort),
            "Guaranteed" => Some(QosClass::Guaranteed),
            _ => None,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct KubePodInfoV1<'a> {
    pub namespace: Option<&'a str>,
    pub name: &'a str,
    pub creation_timestamp: Option<f64>,
    pub labels: BTreeMap<&'a str, LabelRef<'a>>,
    /// Annotations filtered with user input.
    ///
    /// After receiving the annotations from the Kubernetes API, we cannot
    /// process all of them as HostLabels. FilteredAnnotations are those
    /// annotations, which can be processed. This means that the annotations can
    /// no longer be arbitrary json objects and that options from the
    /// `Kubernetes` rule have been taken into account.
    pub annotations: BTreeMap<&'a str, &'a str>,
    // A pod might not be scheduled (yet) on a node (e.g. resource constraints)
    pub node: Option<&'a str>,
    pub host_network: Option<bool>,
    pub dns_policy: Option<&'a str>,
    pub host_ip: Option<&'a str>,
    pub pod_ip: Option<&'a str>,
    pub qos_class: Option<QosClass>,
    pub restart_policy: &'a str,
    pub uid: &'a str,
    pub controllers: Vec<Controller<'a>>,
    pub cluster: &'a str,
    pub kubernetes_cluster_hostname: &'a str,
}

impl Section for KubePodInfoV1<'_> {
    const NAME: &'static str = "kube_pod_info_v1";
}

/// The Kubernetes Pod lifecycle phase. (`kube_pod_lifecycle_v1`)
///
/// According to upstream [documentation], this information from the Pod status
/// is guaranteed to be one of the values: `Pending`, `Running`, `Succeeded`,
/// `Failed`, or `Unknown`.
///
/// [documentation]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/
#[derive(Debug, Serialize)]
pub(crate) struct KubePodLifecycleV1 {
    phase: String,
}

impl KubePodLifecycleV1 {
    /// Create a section given the Pod phase from the Kubernetes API.
    ///
    /// Note that the Kubernetes API provides the Pod phase as a title-case
    /// string and we explicitly convert it to its lowercase equivalent, as this
    /// is what the existing wire protocol expects.
    ///
    /// Also note that aside from converting to lowercase, we make no attempt to
    /// validate the phase here, instead relying on the check plugin to decide
    /// what to do (e.g. crash so we can get a crash report and add levels for
    /// a new phase or similar).
    pub fn new(phase: &str) -> Self {
        Self {
            phase: phase.to_lowercase(),
        }
    }
}

impl Section for KubePodLifecycleV1 {
    const NAME: &'static str = "kube_pod_lifecycle_v1";
}
