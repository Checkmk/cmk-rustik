use k8s_openapi::api::core::v1::Node;
use serde::Serialize;
use std::sync::Arc;

use crate::host_settings::{HostSettings, node_roles};
use crate::ingest::api_health;
use crate::section::Section;

/// Cluster info. (`kube_cluster_info_v1`)
#[derive(Serialize)]
pub(crate) struct KubeClusterInfoV1<'a> {
    pub name: &'a str,
    pub version: &'a str,
}

impl<'a> KubeClusterInfoV1<'a> {
    pub(crate) fn from_host_settings(settings: &'a HostSettings) -> KubeClusterInfoV1<'a> {
        KubeClusterInfoV1 {
            name: settings.cluster_name.as_str(),
            version: settings.cluster_version.as_str(),
        }
    }
}

impl Section for KubeClusterInfoV1<'_> {
    const NAME: &'static str = "kube_cluster_info_v1";
}

/// Cluster details. (`kube_cluster_details_v1`)
///
/// Provides API health (result of polling `/readyz` and `/livez` using the
/// Kubernetes API client).
#[derive(Serialize)]
pub(crate) struct KubeClusterDetailsV1<'a> {
    api_health: ApiHealth<'a>,
}

#[derive(Serialize)]
pub(crate) struct ApiHealth<'a> {
    live: ApiHealthResponse<'a>,
    ready: ApiHealthResponse<'a>,
}

#[derive(Serialize)]
pub(crate) struct ApiHealthResponse<'a> {
    status_code: u16,
    response: &'a str,
}

impl<'a> From<&'a api_health::HealthResponse> for ApiHealthResponse<'a> {
    fn from(response: &'a api_health::HealthResponse) -> ApiHealthResponse<'a> {
        Self {
            status_code: response.status_code,
            response: &response.body,
        }
    }
}

impl<'a> KubeClusterDetailsV1<'a> {
    pub fn new(update: &'a api_health::ApiHealthUpdate) -> Option<Self> {
        let Some(health) = update else {
            return None;
        };
        let live = &health.live;
        let ready = &health.ready;
        Some(Self {
            api_health: ApiHealth {
                live: live.into(),
                ready: ready.into(),
            },
        })
    }
}

impl Section for KubeClusterDetailsV1<'_> {
    const NAME: &'static str = "kube_cluster_details_v1";
}

/// One node's contribution to the node count.
#[derive(Serialize)]
struct CountableNode<'a> {
    ready: bool,
    roles: Vec<&'a str>,
}

/// Ready/not-ready node counts, split by role. (`kube_node_count_v1`)
#[derive(Serialize)]
pub(crate) struct KubeNodeCountV1<'a> {
    nodes: Vec<CountableNode<'a>>,
}

impl<'a> KubeNodeCountV1<'a> {
    pub fn from_nodes(nodes: &'a [Arc<Node>]) -> KubeNodeCountV1<'a> {
        KubeNodeCountV1 {
            nodes: nodes
                .iter()
                .map(|node| CountableNode {
                    ready: node_is_ready(node),
                    roles: node_roles(node).collect(),
                })
                .collect(),
        }
    }
}

impl Section for KubeNodeCountV1<'_> {
    const NAME: &'static str = "kube_node_count_v1";
}

/// A node is ready when the first of its conditions of type `Ready` has status
/// `True`.
fn node_is_ready(node: &Node) -> bool {
    node.status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .into_iter()
        .flatten()
        .find(|condition| condition.type_.eq_ignore_ascii_case("ready"))
        .is_some_and(|condition| condition.status == "True")
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{NodeCondition, NodeStatus};
    use std::collections::BTreeMap;

    use crate::test_support::{host_settings, node, node_with_roles, s};

    fn condition(type_: &str, status: &str) -> NodeCondition {
        NodeCondition {
            type_: s(type_),
            status: s(status),
            ..Default::default()
        }
    }

    /// A node with the given roles, reporting the given `Ready` condition
    /// status.
    fn countable_node(name: &str, roles: &[&str], ready: &str) -> Node {
        let mut node = node_with_roles(name, roles);
        node.status = Some(NodeStatus {
            conditions: Some(vec![condition("Ready", ready)]),
            ..Default::default()
        });
        node
    }

    #[test]
    fn kube_cluster_info_v1() {
        insta::assert_json_snapshot!(KubeClusterInfoV1::from_host_settings(&host_settings()));
    }

    #[test]
    fn kube_cluster_details_v1() {
        let api_health = api_health::ApiHealth {
            live: api_health::HealthResponse {
                status_code: 200,
                body: s("ok"),
            },
            ready: api_health::HealthResponse {
                status_code: 200,
                body: s("ok"),
            },
        };
        insta::assert_json_snapshot!(KubeClusterDetailsV1::new(&Some(api_health.into())));
    }

    #[test]
    fn no_cluster_details_without_api_health() {
        assert!(KubeClusterDetailsV1::new(&None).is_none());
    }

    #[test]
    fn kube_node_count_v1() {
        let nodes = [
            countable_node("control-1", &["control-plane", "master"], "True"),
            countable_node("worker-1", &["worker"], "True"),
            countable_node("worker-2", &["worker"], "False"),
            // A node need not have a role at all
            countable_node("unlabelled-1", &[], "Unknown"),
        ]
        .map(Arc::new);
        insta::assert_json_snapshot!(KubeNodeCountV1::from_nodes(&nodes));
    }

    /// A cluster we know nothing about yet still emits the section
    #[test]
    fn kube_node_count_v1_without_nodes() {
        assert!(KubeNodeCountV1::from_nodes(&[]).nodes.is_empty());
    }

    /// Kubernetes condition types are conventionally capitalized, but the
    /// Python section matched them case-insensitively, so we do too.
    #[test]
    fn kube_node_count_v1_readiness_is_case_insensitive() {
        let mut node = node("node01");
        node.status = Some(NodeStatus {
            conditions: Some(vec![condition("ready", "True")]),
            ..Default::default()
        });
        let nodes = [Arc::new(node)];
        assert!(KubeNodeCountV1::from_nodes(&nodes).nodes[0].ready);
    }

    /// Roles come from the `node-role.kubernetes.io/` labels only
    #[test]
    fn kube_node_count_v1_roles() {
        let mut node = node("node01");
        node.metadata.labels = Some(BTreeMap::from([
            (s("node-role.kubernetes.io/control-plane"), s("")),
            (s("node-role.kubernetes.io/worker"), s("")),
            (s("kubernetes.io/arch"), s("amd64")),
        ]));
        let nodes = [Arc::new(node)];
        assert_eq!(
            KubeNodeCountV1::from_nodes(&nodes).nodes[0].roles,
            vec!["control-plane", "worker"]
        );
    }
}
