use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::{
    Section,
    common::{Controller, LabelRef},
};
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

fn to_unix_seconds(time: &Time) -> i64 {
    time.0.as_millisecond() / 1000
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub(crate) enum ContainerStateValue<'a> {
    #[serde(rename = "running")]
    Running { start_time: Option<i64> },
    #[serde(rename = "waiting")]
    Waiting {
        reason: Option<&'a str>,
        detail: Option<&'a str>,
    },
    #[serde(rename = "terminated")]
    Terminated {
        exit_code: i32,
        start_time: Option<i64>,
        end_time: Option<i64>,
        reason: Option<&'a str>,
        detail: Option<&'a str>,
    },
}

#[derive(Serialize)]
pub(crate) struct ContainerStatusValue<'a> {
    pub container_id: Option<&'a str>,
    pub image_id: &'a str,
    pub name: &'a str,
    pub image: &'a str,
    pub ready: bool,
    pub state: ContainerStateValue<'a>,
    pub restart_count: i32,
}

impl<'a> ContainerStatusValue<'a> {
    fn from_status(status: &'a ContainerStatus) -> Option<Self> {
        let state = status.state.as_ref()?;
        let state = if let Some(running) = &state.running {
            ContainerStateValue::Running {
                start_time: running.started_at.as_ref().map(to_unix_seconds),
            }
        } else if let Some(waiting) = &state.waiting {
            ContainerStateValue::Waiting {
                reason: waiting.reason.as_deref(),
                detail: waiting.message.as_deref(),
            }
        } else if let Some(terminated) = &state.terminated {
            ContainerStateValue::Terminated {
                exit_code: terminated.exit_code,
                start_time: terminated.started_at.as_ref().map(to_unix_seconds),
                end_time: terminated.finished_at.as_ref().map(to_unix_seconds),
                reason: terminated.reason.as_deref(),
                detail: terminated.message.as_deref(),
            }
        } else {
            return None;
        };

        Some(Self {
            container_id: status.container_id.as_deref(),
            image_id: &status.image_id,
            name: &status.name,
            image: &status.image,
            ready: status.ready,
            state,
            restart_count: status.restart_count,
        })
    }
}

/// Pod container statuses. (`kube_pod_containers_v1`)
#[derive(Serialize)]
pub(crate) struct KubePodContainersV1<'a> {
    pub containers: BTreeMap<&'a str, ContainerStatusValue<'a>>,
}

impl<'a> KubePodContainersV1<'a> {
    pub(crate) fn from_pod(pod: &'a Pod) -> Option<Self> {
        let statuses = pod.status.as_ref()?.container_statuses.as_ref()?;
        let mut containers = BTreeMap::new();
        for status in statuses {
            if let Some(value) = ContainerStatusValue::from_status(status) {
                containers.insert(status.name.as_str(), value);
            }
        }
        if containers.is_empty() {
            return None;
        }
        Some(Self { containers })
    }
}

impl Section for KubePodContainersV1<'_> {
    const NAME: &'static str = "kube_pod_containers_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use k8s_openapi::api::core::v1::{
        ContainerState, ContainerStateRunning, ContainerStateTerminated, ContainerStateWaiting,
    };
    use k8s_openapi::jiff::Timestamp;

    use crate::test_support::{owner_graph, owner_ref, pod, pod_prefilled};

    fn container_status(name: &str, state: ContainerState) -> ContainerStatus {
        ContainerStatus {
            container_id: Some(format!("containerd://{name}")),
            image_id: format!("{name}-image-id"),
            name: name.to_string(),
            image: format!("{name}:latest"),
            ready: true,
            state: Some(state),
            restart_count: 0,
            ..Default::default()
        }
    }

    fn timestamp(s: &str) -> Time {
        let timestamp: Timestamp = s.parse().unwrap();
        Time(timestamp)
    }

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
        let settings = HostSettings {
            cluster_name: "test-cluster".to_string(),
            cluster_host_name: "test-host".to_string(),
            annotation_key_pattern: crate::host_settings::AnnotationKeyPattern::IgnoreAll,
            excluded_node_role_patterns: Vec::new(),
            always_emitted: crate::host_settings::AlwaysEmitted::default(),
        };
        insta::assert_json_snapshot!(KubePodInfoV1::from_pod(&pod, &graph, &settings));
    }

    #[test]
    fn kube_pod_containers_v1() {
        let mut pod = pod_prefilled("my-pod");
        pod.status
            .get_or_insert_with(Default::default)
            .container_statuses = Some(vec![
            container_status(
                "nginx",
                ContainerState {
                    running: Some(ContainerStateRunning {
                        started_at: Some(timestamp("2024-06-19 15:22:45-04")),
                    }),
                    ..Default::default()
                },
            ),
            container_status(
                "sidecar",
                ContainerState {
                    waiting: Some(ContainerStateWaiting {
                        reason: Some("PodInitializing".to_string()),
                        message: None,
                    }),
                    ..Default::default()
                },
            ),
        ]);
        insta::assert_json_snapshot!(KubePodContainersV1::from_pod(&pod));
    }

    #[test]
    fn kube_pod_containers_v1_terminated() {
        let mut pod = pod_prefilled("my-pod");
        pod.status
            .get_or_insert_with(Default::default)
            .container_statuses = Some(vec![container_status(
            "nginx",
            ContainerState {
                terminated: Some(ContainerStateTerminated {
                    exit_code: 1,
                    started_at: Some(timestamp("2024-06-19 15:22:45-04")),
                    finished_at: Some(timestamp("2024-06-19 15:23:00-04")),
                    reason: Some("Error".to_string()),
                    message: Some("boom".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )]);
        insta::assert_json_snapshot!(KubePodContainersV1::from_pod(&pod));
    }

    #[test]
    fn kube_pod_containers_v1_without_state() {
        let mut pod = pod_prefilled("my-pod");
        pod.status
            .get_or_insert_with(Default::default)
            .container_statuses = Some(vec![ContainerStatus {
            state: None,
            ..container_status("nginx", ContainerState::default())
        }]);
        assert!(KubePodContainersV1::from_pod(&pod).is_none());
    }

    #[test]
    fn kube_pod_containers_v1_without_containers() {
        assert!(KubePodContainersV1::from_pod(&pod("my-pod", None)).is_none());
    }
}
