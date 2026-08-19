use k8s_openapi::api::core::v1::Pod;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::{Controller, LabelRef};
use crate::snapshot::owner_graph::OwnerGraph;

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

/// Pod info. (`kube_pod_info_v1`)
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

impl<'a> KubePodInfoV1<'a> {
    pub(crate) fn from_pod(
        pod: &'a Pod,
        owner_graph: &'a OwnerGraph,
        settings: &'a HostSettings,
    ) -> Option<KubePodInfoV1<'a>> {
        let control_chain = match &pod.metadata.uid {
            Some(uid) => owner_graph
                .walk_up(uid)
                .into_iter()
                .map(|o| Controller {
                    type_: &o.kind,
                    name: &o.name,
                })
                .collect(),
            None => Vec::new(),
        };

        let section = KubePodInfoV1 {
            namespace: pod.metadata.namespace.as_deref(),
            name: pod.metadata.name.as_deref()?,
            creation_timestamp: pod
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: pod
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: pod
                .metadata
                .annotations
                .as_ref()
                .map(|m| settings.annotation_key_pattern.filter(m))
                .unwrap_or_default(),
            node: pod.spec.as_ref().and_then(|s| s.node_name.as_deref()),
            host_network: pod.spec.as_ref().and_then(|s| s.host_network),
            dns_policy: pod.spec.as_ref().and_then(|s| s.dns_policy.as_deref()),
            host_ip: pod.status.as_ref().and_then(|s| s.host_ip.as_deref()),
            pod_ip: pod.status.as_ref().and_then(|s| s.pod_ip.as_deref()),
            qos_class: pod
                .status
                .as_ref()
                .and_then(|s| s.qos_class.as_deref())
                .and_then(QosClass::from_str),
            restart_policy: pod
                .spec
                .as_ref()
                .and_then(|s| s.restart_policy.as_deref())
                .unwrap_or("Always"),
            uid: pod.metadata.uid.as_deref().unwrap_or_default(),
            controllers: control_chain,
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        };

        Some(section)
    }
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

/// Kubernetes Pod start time. (`kube_start_time_v1`)
///
/// Date and time at which the object was acknowledged by the Kubelet,
/// the check plugin turns it into an uptime. The Kubernetes API leaves
/// `.status.startTime` unset until then (e.g. for a Pod that cannot be
/// scheduled and is Pending), so this section is conditional.
#[derive(Debug, Serialize)]
pub(crate) struct KubeStartTimeV1 {
    start_time: f64,
}

impl KubeStartTimeV1 {
    /// Create a section for a Pod, unless the Pod has no start time yet.
    pub(crate) fn from_pod(pod: &Pod) -> Option<Self> {
        let start_time = pod.status.as_ref()?.start_time.as_ref()?;
        Some(Self {
            start_time: start_time.0.as_millisecond() as f64 / 1000.0,
        })
    }
}

impl Section for KubeStartTimeV1 {
    const NAME: &'static str = "kube_start_time_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{host_settings, owner_graph, owner_ref, pod, pod_prefilled};

    #[test]
    fn kube_pod_lifecycle_v1() {
        insta::assert_json_snapshot!(KubePodLifecycleV1::new("Pending"));
        insta::assert_json_snapshot!(KubePodLifecycleV1::new("Running"));
    }

    #[test]
    fn kube_pod_info_v1() {
        let pod = pod_prefilled("my-pod");
        let graph = owner_graph(&[
            (
                pod.metadata.uid.as_ref().unwrap(),
                owner_ref("ReplicaSet", "rs-1", "rs-uid"),
            ),
            ("rs-uid", owner_ref("Deployment", "deploy-1", "deploy-uid")),
        ]);
        let settings = host_settings();
        insta::assert_json_snapshot!(KubePodInfoV1::from_pod(&pod, &graph, &settings));
    }

    #[test]
    fn kube_start_time_v1() {
        let section = KubeStartTimeV1::from_pod(&pod_prefilled("my-pod"));
        insta::assert_json_snapshot!(section);
    }

    #[test]
    fn kube_start_time_v1_is_none_without_a_start_time() {
        // A Pod without any status
        assert!(KubeStartTimeV1::from_pod(&pod("unscheduled", None)).is_none());

        // A Pod which has a status, but no start time in it
        let mut without_start_time = pod_prefilled("my-pod");
        without_start_time.status.as_mut().unwrap().start_time = None;
        assert!(KubeStartTimeV1::from_pod(&without_start_time).is_none());
    }
}
