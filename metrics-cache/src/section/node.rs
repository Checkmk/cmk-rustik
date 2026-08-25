use k8s_openapi::api::core::v1::{Node, Pod};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use serde::Serialize;
use std::collections::BTreeMap;
use std::ops::Add;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::{LabelRef, parse_quantity};

/// One entry of a Node's `status.addresses`.
///
/// Not `k8s_openapi`'s `NodeAddress`: that one serializes as `type`, while
/// Checkmk expects `type_`.
#[derive(Serialize)]
pub(crate) struct NodeAddressRef<'a> {
    address: &'a str,
    type_: &'a str,
}

/// Node info. (`kube_node_info_v1`)
#[derive(Serialize)]
pub(crate) struct KubeNodeInfoV1<'a> {
    pub architecture: &'a str,
    pub kernel_version: &'a str,
    pub os_image: &'a str,
    pub operating_system: &'a str,
    pub container_runtime_version: &'a str,
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
    pub addresses: Vec<NodeAddressRef<'a>>,
    pub cluster: &'a str,
    pub kubernetes_cluster_hostname: &'a str,
}

impl<'a> KubeNodeInfoV1<'a> {
    pub fn from_node(node: &'a Node, settings: &'a HostSettings) -> Option<KubeNodeInfoV1<'a>> {
        let status = node.status.as_ref()?;
        let node_info = status.node_info.as_ref()?;
        let node_section = KubeNodeInfoV1 {
            architecture: &node_info.architecture,
            kernel_version: &node_info.kernel_version,
            os_image: &node_info.os_image,
            operating_system: &node_info.operating_system,
            container_runtime_version: &node_info.container_runtime_version,
            name: node.metadata.name.as_deref()?,
            creation_timestamp: node
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|x| x.0.as_millisecond() as f64 / 1000.0),
            labels: node
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: node
                .metadata
                .annotations
                .as_ref()
                .map(|x| settings.annotation_key_pattern.filter(x))
                .unwrap_or_default(),
            addresses: status
                .addresses
                .iter()
                .flatten()
                .map(|x| NodeAddressRef {
                    address: &x.address,
                    type_: &x.type_,
                })
                .collect(),
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        };
        Some(node_section)
    }
}

impl Section for KubeNodeInfoV1<'_> {
    const NAME: &'static str = "kube_node_info_v1";
}

/// Container counts by state. (`kube_node_container_count_v1`)
#[derive(Default, Serialize)]
pub(crate) struct KubeNodeContainerCountV1 {
    pub running: u64,
    pub waiting: u64,
    pub terminated: u64,
}

impl KubeNodeContainerCountV1 {
    pub fn new<'a>(pods: impl IntoIterator<Item = &'a Arc<Pod>>) -> Self {
        let mut section = Self::default();

        for pod in pods {
            let Some(statuses) = pod
                .status
                .as_ref()
                .and_then(|status| status.container_statuses.as_deref())
            else {
                continue;
            };
            for status in statuses {
                let Some(state) = &status.state else {
                    continue;
                };
                if state.terminated.is_some() {
                    section.terminated += 1;
                    continue;
                }
                if state.running.is_some() {
                    section.running += 1;
                    continue;
                }
                // Upstream says: "If none of them is specified, the default one
                // is ContainerStateWaiting.", so if we are still here whether
                // or not status.waiting is not None, we count it as waiting.
                section.waiting += 1;
            }
        }

        section
    }
}

impl Section for KubeNodeContainerCountV1 {
    const NAME: &'static str = "kube_node_container_count_v1";
}

#[derive(Default, Serialize)]
pub(crate) struct KubeAllocatablePodsV1 {
    pub capacity: u64,
    pub allocatable: u64,
}

fn pod_count(resources: Option<&BTreeMap<String, Quantity>>) -> u64 {
    resources
        .and_then(|r| r.get("pods"))
        .and_then(|q| parse_quantity(&q.0))
        .map_or(0, |v| v.ceil() as u64)
}

impl KubeAllocatablePodsV1 {
    pub fn from_node(node: &Node) -> Self {
        let status = node.status.as_ref();
        Self {
            capacity: pod_count(status.and_then(|s| s.capacity.as_ref())),
            allocatable: pod_count(status.and_then(|s| s.allocatable.as_ref())),
        }
    }

    pub fn from_nodes<'a>(nodes: impl IntoIterator<Item = &'a Node>) -> Self {
        nodes
            .into_iter()
            .map(Self::from_node)
            .fold(Self::default(), Add::add)
    }
}

impl Add for KubeAllocatablePodsV1 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            capacity: self.capacity + rhs.capacity,
            allocatable: self.allocatable + rhs.allocatable,
        }
    }
}

impl Section for KubeAllocatablePodsV1 {
    const NAME: &'static str = "kube_allocatable_pods_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{
        ContainerState, ContainerStateRunning, ContainerStateTerminated, ContainerStateWaiting,
        ContainerStatus, NodeStatus,
    };
    use regex::Regex;

    use crate::host_settings::AnnotationKeyPattern;
    use crate::test_support::{host_settings, node, node_prefilled, pod_prefilled, s};

    fn node_with_pod_counts(capacity: &str, allocatable: &str) -> Node {
        let mut node = node("worker-1");
        node.status = Some(NodeStatus {
            capacity: Some(BTreeMap::from([(s("pods"), Quantity(s(capacity)))])),
            allocatable: Some(BTreeMap::from([(s("pods"), Quantity(s(allocatable)))])),
            ..Default::default()
        });
        node
    }

    fn pod_with_container_states(states: Vec<ContainerState>) -> Arc<Pod> {
        let mut pod = pod_prefilled("pod");
        pod.status.as_mut().unwrap().container_statuses = Some(
            states
                .into_iter()
                .enumerate()
                .map(|(i, state)| ContainerStatus {
                    name: format!("container-{i}"),
                    state: Some(state),
                    ..Default::default()
                })
                .collect(),
        );
        Arc::new(pod)
    }

    #[test]
    fn kube_node_info_v1() {
        let node = node_prefilled("worker-1");
        let mut settings = host_settings();
        insta::assert_json_snapshot!(KubeNodeInfoV1::from_node(&node, &settings));

        // This pattern should only match one annotation
        settings.annotation_key_pattern =
            AnnotationKeyPattern::Pattern(Regex::new("^example").unwrap());
        assert_eq!(
            KubeNodeInfoV1::from_node(&node, &settings)
                .unwrap()
                .annotations
                .len(),
            1
        );

        // Ignore all annotations, emit 0 of them
        settings.annotation_key_pattern = AnnotationKeyPattern::IgnoreAll;
        assert_eq!(
            KubeNodeInfoV1::from_node(&node, &settings)
                .unwrap()
                .annotations
                .len(),
            0
        );

        // Import all annotations (fixture default, captured by insta above, but let's be explicit)
        settings.annotation_key_pattern = AnnotationKeyPattern::ImportAll;
        assert_eq!(
            KubeNodeInfoV1::from_node(&node, &settings)
                .unwrap()
                .annotations
                .len(),
            2
        );
    }

    /// A Node without `status.nodeInfo` cannot fill the five mandatory fields
    /// of the section, so we emit nothing rather than something unparseable.
    #[test]
    fn kube_node_info_v1_without_node_info() {
        assert!(KubeNodeInfoV1::from_node(&node("worker-1"), &host_settings()).is_none());
    }

    #[test]
    fn kube_node_container_count_v1_counts_all_pods() {
        let pods = [
            pod_with_container_states(vec![ContainerState {
                running: Some(ContainerStateRunning::default()),
                ..Default::default()
            }]),
            pod_with_container_states(vec![ContainerState {
                terminated: Some(ContainerStateTerminated::default()),
                ..Default::default()
            }]),
        ];

        let count = KubeNodeContainerCountV1::new(&pods);
        assert_eq!((count.running, count.waiting, count.terminated), (1, 0, 1));
    }

    #[test]
    fn kube_node_container_count_v1_defaults_to_waiting() {
        let pods = [pod_with_container_states(vec![ContainerState::default()])];

        let count = KubeNodeContainerCountV1::new(&pods);
        assert_eq!((count.running, count.waiting, count.terminated), (0, 1, 0));
    }

    #[test]
    fn kube_node_container_count_v1_counts_mixed_states() {
        let pods = [pod_with_container_states(vec![
            ContainerState {
                running: Some(ContainerStateRunning::default()),
                ..Default::default()
            },
            ContainerState {
                waiting: Some(ContainerStateWaiting::default()),
                ..Default::default()
            },
            ContainerState {
                terminated: Some(ContainerStateTerminated::default()),
                ..Default::default()
            },
            ContainerState {
                running: Some(ContainerStateRunning::default()),
                ..Default::default()
            },
        ])];

        let count = KubeNodeContainerCountV1::new(&pods);
        assert_eq!((count.running, count.waiting, count.terminated), (2, 1, 1));
    }

    #[test]
    fn kube_allocatable_pods_v1() {
        insta::assert_json_snapshot!(KubeAllocatablePodsV1::from_node(&node_with_pod_counts(
            "110", "110"
        )));
    }

    #[test]
    fn kube_allocatable_pods_v1_without_status_is_zero() {
        let section = KubeAllocatablePodsV1::from_node(&node("worker-1"));
        assert_eq!((section.capacity, section.allocatable), (0, 0));
    }

    #[test]
    fn kube_allocatable_pods_v1_sums_across_nodes() {
        let nodes = [
            node_with_pod_counts("110", "110"),
            node_with_pod_counts("1k", "250.5"),
        ];

        let section = KubeAllocatablePodsV1::from_nodes(&nodes);
        assert_eq!((section.capacity, section.allocatable), (1110, 361));
    }
}
