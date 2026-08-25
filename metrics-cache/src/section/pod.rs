use k8s_openapi::api::core::v1::{Pod, PodCondition};
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::{Controller, LabelRef};
use crate::section::container::{ContainerSpecValue, ContainerStatusValue};
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

/// Pod names grouped by their Kubernetes lifecycle phase.
/// (`kube_pod_resources_v1`)
///
/// The names borrow from the API objects in the current snapshot; this keeps
/// the section cheap even for large clusters.
#[derive(Debug, Default, Serialize)]
pub(crate) struct KubePodResourcesV1<'a> {
    running: Vec<&'a str>,
    pending: Vec<&'a str>,
    succeeded: Vec<&'a str>,
    failed: Vec<&'a str>,
    unknown: Vec<&'a str>,
}

impl<'a> KubePodResourcesV1<'a> {
    /// Group the named pods by their API-reported lifecycle phase.
    ///
    /// Missing or unrecognised phases are retained in `unknown`, while pods
    /// without a name cannot be represented by the wire schema and are skipped.
    pub(crate) fn from_pods(pods: impl IntoIterator<Item = &'a Pod>) -> Self {
        let mut resources = Self::default();
        for pod in pods {
            let Some(name) = pod.metadata.name.as_deref() else {
                continue;
            };
            let phase = pod
                .status
                .as_ref()
                .and_then(|status| status.phase.as_deref());
            match phase {
                Some("Running") => resources.running.push(name),
                Some("Pending") => resources.pending.push(name),
                Some("Succeeded") => resources.succeeded.push(name),
                Some("Failed") => resources.failed.push(name),
                _ => resources.unknown.push(name),
            }
        }
        resources
    }
}

impl Section for KubePodResourcesV1<'_> {
    const NAME: &'static str = "kube_pod_resources_v1";
}

#[derive(Serialize)]
pub(crate) struct PodConditionValue<'a> {
    pub status: &'a str,
    pub reason: Option<&'a str>,
    pub detail: Option<&'a str>,
    pub last_transition_time: Option<i64>,
}

impl<'a> PodConditionValue<'a> {
    fn from_condition(condition: &'a PodCondition) -> Self {
        Self {
            status: &condition.status,
            reason: condition.reason.as_deref(),
            detail: condition.message.as_deref(),
            last_transition_time: condition
                .last_transition_time
                .as_ref()
                .map(|t| t.0.as_millisecond() / 1000),
        }
    }
}

/// Pod conditions. (`kube_pod_conditions_v1`)
#[derive(Serialize)]
pub(crate) struct KubePodConditionsV1<'a> {
    pub initialized: Option<PodConditionValue<'a>>,
    pub hasnetwork: Option<PodConditionValue<'a>>,
    pub readytostartcontainers: Option<PodConditionValue<'a>>,
    pub scheduled: PodConditionValue<'a>,
    pub containersready: Option<PodConditionValue<'a>>,
    pub ready: Option<PodConditionValue<'a>>,
    pub disruptiontarget: Option<PodConditionValue<'a>>,
    pub resizepending: Option<PodConditionValue<'a>>,
    pub resizeinprogress: Option<PodConditionValue<'a>>,
    pub allcontainersrestarting: Option<PodConditionValue<'a>>,
}

impl<'a> KubePodConditionsV1<'a> {
    pub(crate) fn from_pod(pod: &'a Pod) -> Option<Self> {
        let mut initialized = None;
        let mut hasnetwork = None;
        let mut readytostartcontainers = None;
        let mut scheduled = None;
        let mut containersready = None;
        let mut ready = None;
        let mut disruptiontarget = None;
        let mut resizepending = None;
        let mut resizeinprogress = None;
        let mut allcontainersrestarting = None;

        let conditions = pod.status.as_ref()?.conditions.as_ref()?;
        for condition in conditions {
            let value = PodConditionValue::from_condition(condition);
            match condition.type_.as_str() {
                "Initialized" => initialized = Some(value),
                "PodHasNetwork" => hasnetwork = Some(value),
                "PodReadyToStartContainers" => readytostartcontainers = Some(value),
                "PodScheduled" => scheduled = Some(value),
                "ContainersReady" => containersready = Some(value),
                "Ready" => ready = Some(value),
                "DisruptionTarget" => disruptiontarget = Some(value),
                "PodResizePending" => resizepending = Some(value),
                "PodResizeInProgress" => resizeinprogress = Some(value),
                "AllContainersRestarting" => allcontainersrestarting = Some(value),
                _ => {} // unrecognized condition type: dropped, matches Python `from_kube_api`
            }
        }

        Some(Self {
            initialized,
            hasnetwork,
            readytostartcontainers,
            scheduled: scheduled?,
            containersready,
            ready,
            disruptiontarget,
            resizepending,
            resizeinprogress,
            allcontainersrestarting,
        })
    }
}

impl Section for KubePodConditionsV1<'_> {
    const NAME: &'static str = "kube_pod_conditions_v1";
}

/// Pod container statuses. (`kube_pod_containers_v1`)
#[derive(Serialize)]
pub(crate) struct KubePodContainersV1<'a> {
    pub containers: BTreeMap<&'a str, ContainerStatusValue<'a>>,
}

impl<'a> KubePodContainersV1<'a> {
    pub(crate) fn from_pod(pod: &'a Pod) -> Option<Self> {
        let statuses = pod.status.as_ref()?.container_statuses.as_ref()?;
        let containers = ContainerStatusValue::from_statuses(statuses);
        if containers.is_empty() {
            return None;
        }
        Some(Self { containers })
    }
}

impl Section for KubePodContainersV1<'_> {
    const NAME: &'static str = "kube_pod_containers_v1";
}

/// Pod init-container statuses. (`kube_pod_init_containers_v1`)
#[derive(Serialize)]
pub(crate) struct KubePodInitContainersV1<'a> {
    pub containers: BTreeMap<&'a str, ContainerStatusValue<'a>>,
}

impl<'a> KubePodInitContainersV1<'a> {
    pub(crate) fn from_pod(pod: &'a Pod) -> Option<Self> {
        let statuses = pod.status.as_ref()?.init_container_statuses.as_ref()?;
        let containers = ContainerStatusValue::from_statuses(statuses);
        if containers.is_empty() {
            return None;
        }
        Some(Self { containers })
    }
}

impl Section for KubePodInitContainersV1<'_> {
    const NAME: &'static str = "kube_pod_init_containers_v1";
}

/// Pod container specs. (`kube_pod_container_specs_v1`)
#[derive(Serialize)]
pub(crate) struct KubePodContainerSpecsV1<'a> {
    pub containers: BTreeMap<&'a str, ContainerSpecValue<'a>>,
}

impl<'a> KubePodContainerSpecsV1<'a> {
    pub(crate) fn from_pod(pod: &'a Pod) -> Option<Self> {
        let containers = &pod.spec.as_ref()?.containers;
        Some(Self {
            containers: ContainerSpecValue::from_specs(containers)?,
        })
    }
}

impl Section for KubePodContainerSpecsV1<'_> {
    const NAME: &'static str = "kube_pod_container_specs_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use k8s_openapi::api::core::v1::{
        ContainerState, ContainerStateRunning, ContainerStateTerminated, ContainerStateWaiting,
        ContainerStatus,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
    use k8s_openapi::jiff::Timestamp;

    use crate::test_support::{
        container, host_settings, owner_graph, owner_ref, pod, pod_prefilled, s,
    };

    fn condition(type_: &str, status: &str, reason: &str, message: &str) -> PodCondition {
        let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
        PodCondition {
            type_: type_.to_string(),
            status: status.to_string(),
            reason: Some(reason.to_string()),
            message: Some(message.to_string()),
            last_transition_time: Some(Time(timestamp)),
            ..Default::default()
        }
    }

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

    #[test]
    fn kube_pod_resources_v1() {
        let pod_with_phase = |name, phase| {
            let mut pod = pod(name, None);
            pod.status.get_or_insert_with(Default::default).phase = Some(s(phase));
            pod
        };
        let running = pod_with_phase("running-pod", "Running");
        let running2 = pod_with_phase("running-2-pod", "Running");
        let pending = pod_with_phase("pending-pod", "Pending");
        let succeeded = pod_with_phase("succeeded-pod", "Succeeded");
        let failed = pod_with_phase("failed-pod", "Failed");
        let failed2 = pod_with_phase("failed-2-pod", "Failed");
        let unknown = pod("no-phase-pod", None);

        let section = KubePodResourcesV1::from_pods([
            &running, &pending, &succeeded, &failed, &unknown, &running2, &failed2,
        ]);
        insta::assert_json_snapshot!(section);
    }

    #[test]
    fn kube_pod_conditions_v1() {
        let mut pod = pod_prefilled("my-pod");
        pod.status.get_or_insert_with(Default::default).conditions = Some(vec![
            condition("PodScheduled", "True", "", ""),
            condition("Initialized", "True", "", ""),
            condition(
                "Ready",
                "False",
                "ContainersNotReady",
                "containers not ready",
            ),
        ]);
        insta::assert_json_snapshot!(KubePodConditionsV1::from_pod(&pod));
    }

    #[test]
    fn kube_pod_conditions_v1_ignores_unknown_condition_types() {
        let mut pod = pod_prefilled("my-pod");
        pod.status.get_or_insert_with(Default::default).conditions = Some(vec![
            condition("PodScheduled", "True", "", ""),
            condition("SomeFutureCondition", "True", "", ""),
        ]);
        insta::assert_json_snapshot!(KubePodConditionsV1::from_pod(&pod));
    }

    #[test]
    fn kube_pod_conditions_v1_without_scheduled() {
        let mut pod = pod_prefilled("my-pod");
        pod.status.get_or_insert_with(Default::default).conditions =
            Some(vec![condition("Initialized", "True", "", "")]);
        assert!(KubePodConditionsV1::from_pod(&pod).is_none());
    }

    #[test]
    fn kube_pod_conditions_v1_without_conditions() {
        assert!(KubePodConditionsV1::from_pod(&pod("my-pod", None)).is_none());
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
    fn kube_pod_containers_v1_running_without_started_at_is_skipped() {
        let mut pod = pod_prefilled("my-pod");
        pod.status
            .get_or_insert_with(Default::default)
            .container_statuses = Some(vec![container_status(
            "nginx",
            ContainerState {
                running: Some(ContainerStateRunning { started_at: None }),
                ..Default::default()
            },
        )]);
        assert!(KubePodContainersV1::from_pod(&pod).is_none());
    }

    #[test]
    fn kube_pod_containers_v1_without_containers() {
        assert!(KubePodContainersV1::from_pod(&pod("my-pod", None)).is_none());
    }

    #[test]
    fn kube_pod_init_containers_v1() {
        let mut pod = pod_prefilled("my-pod");
        pod.status
            .get_or_insert_with(Default::default)
            .init_container_statuses = Some(vec![
            container_status(
                "init-a",
                ContainerState {
                    running: Some(ContainerStateRunning {
                        started_at: Some(timestamp("2024-06-19 15:22:45-04")),
                    }),
                    ..Default::default()
                },
            ),
            container_status(
                "init-b",
                ContainerState {
                    waiting: Some(ContainerStateWaiting {
                        reason: Some("PodInitializing".to_string()),
                        message: None,
                    }),
                    ..Default::default()
                },
            ),
        ]);
        insta::assert_json_snapshot!(KubePodInitContainersV1::from_pod(&pod));
    }

    #[test]
    fn kube_pod_container_specs_v1() {
        let mut pod = pod_prefilled("my-pod");
        let mut nginx = container("nginx");
        nginx.image_pull_policy = Some(s("Always"));
        let mut sidecar = container("sidecar");
        sidecar.image_pull_policy = Some(s("IfNotPresent"));
        pod.spec.as_mut().unwrap().containers = vec![nginx, sidecar];
        insta::assert_json_snapshot!(KubePodContainerSpecsV1::from_pod(&pod));
    }

    #[test]
    fn kube_pod_container_specs_v1_is_none_if_any_container_is_missing_image_pull_policy() {
        let mut pod = pod_prefilled("my-pod");
        let mut nginx = container("nginx");
        nginx.image_pull_policy = Some(s("Always"));
        let sidecar = container("sidecar");
        pod.spec.as_mut().unwrap().containers = vec![nginx, sidecar];
        assert!(KubePodContainerSpecsV1::from_pod(&pod).is_none());
    }

    #[test]
    fn kube_pod_container_specs_v1_without_spec() {
        let mut pod = pod_prefilled("my-pod");
        pod.spec = None;
        assert!(KubePodContainerSpecsV1::from_pod(&pod).is_none());
    }
}
