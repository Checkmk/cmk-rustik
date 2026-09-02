use k8s_openapi::api::core::v1::{Node, Pod};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::collections::HashSet;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, PiggybackHost};
use crate::section::cluster::{KubeClusterDetailsV1, KubeClusterInfoV1, KubeNodeCountV1};
use crate::section::node::KubeAllocatablePodsV1;
use crate::section::resource::{KubeAllocatableCpuResourceV1, KubeAllocatableMemoryResourceV1};
use crate::section::self_health::KubeRustikHealthV1;
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct Cluster<'a> {
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
    aggregation_nodes: Vec<&'a Arc<Node>>,
    aggregation_pods: Vec<&'a Arc<Pod>>,
}

impl<'a> Cluster<'a> {
    /// Create a new cluster piggyback host instance.
    ///
    /// Importantly, nodes considered here are potentially filtered by user
    /// preference and the node role patterns to exclude are specified as a CLI
    /// argument and land in the [`HostSettings`] `excluded_node_role_patterns`
    /// field.
    ///
    /// This impacts both the nodes that are considered here and pod
    /// aggregations.
    ///
    /// For purposes of aggregation/roll-up, pods are only considered if their
    /// node name is `None` or if their node is not excluded.
    pub fn new(snapshot: &'a Snapshot, settings: &'a HostSettings) -> Cluster<'a> {
        let aggregation_nodes: Vec<&Arc<Node>> =
            Self::aggregation_nodes(&snapshot.stores.nodes, settings);

        // if the node name of the pod is None, keep it. If it is Some, check if it is in aggregation_nodes.
        let aggregation_pods: Vec<&Arc<Pod>> =
            Self::aggregation_pods(&snapshot.stores.pods, &aggregation_nodes);

        Cluster {
            snapshot,
            settings,
            aggregation_nodes,
            aggregation_pods,
        }
    }

    fn aggregation_nodes(nodes: &'a [Arc<Node>], settings: &'a HostSettings) -> Vec<&'a Arc<Node>> {
        nodes
            .iter()
            .filter(|n| !settings.is_node_excluded(n))
            .collect()
    }

    fn aggregation_pods(
        pods: &'a [Arc<Pod>],
        aggregation_nodes: &[&'a Arc<Node>],
    ) -> Vec<&'a Arc<Pod>> {
        let aggregation_node_names: HashSet<&str> = aggregation_nodes
            .iter()
            .filter_map(|n| n.metadata.name.as_deref())
            .collect();
        pods.iter()
            .filter(
                |p| match p.spec.as_ref().and_then(|s| s.node_name.as_deref()) {
                    None => true,
                    Some(name) => aggregation_node_names.contains(&name),
                },
            )
            .collect()
    }
}

impl AggregationHost for Cluster<'_> {
    fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    fn pods(&self) -> impl Iterator<Item = &Arc<Pod>> {
        self.aggregation_pods.iter().copied()
    }
}

impl PiggybackHost for Cluster<'_> {
    fn metadata(&self) -> Option<&ObjectMeta> {
        None
    }

    fn kind(&self) -> &str {
        "cluster"
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = "";
        let mut out = Vec::new();
        out.push(WriteableSection::of(
            me,
            &KubeClusterInfoV1::from_host_settings(self.settings),
        ));
        out.push(WriteableSection::of(
            me,
            // Deliberately every node, not just the aggregation nodes.
            &KubeNodeCountV1::from_nodes(&self.snapshot.stores.nodes),
        ));
        out.extend(self.aggregation_sections(me));
        out.push(WriteableSection::of(
            me,
            &KubeAllocatablePodsV1::from_nodes(
                self.aggregation_nodes.iter().copied().map(Arc::as_ref),
            ),
        ));
        out.push(WriteableSection::of(
            me,
            &KubeAllocatableCpuResourceV1::from_nodes(
                self.aggregation_nodes.iter().copied().map(Arc::as_ref),
            ),
        ));
        out.push(WriteableSection::of(
            me,
            &KubeAllocatableMemoryResourceV1::from_nodes(
                self.aggregation_nodes.iter().copied().map(Arc::as_ref),
            ),
        ));
        out.push(WriteableSection::of(
            me,
            &KubeRustikHealthV1::from_self_health(&self.snapshot.self_health),
        ));
        if let Some(kube_cluster_details_v1) =
            KubeClusterDetailsV1::new(&self.snapshot.api_health_update)
        {
            out.push(WriteableSection::of(me, &kube_cluster_details_v1));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    use crate::section::Section;
    use crate::section::writeable::SectionBody;
    use crate::state::tests::test_app_state;
    use crate::test_support::*;

    fn snapshot_with_nodes(nodes: Vec<Arc<Node>>) -> Snapshot {
        let state = test_app_state();
        let mut snapshot = state.snapshot();
        snapshot.stores.nodes = nodes;
        snapshot
    }

    #[tokio::test]
    async fn test_node_count_covers_nodes_excluded_from_aggregation() {
        let settings = HostSettings {
            excluded_node_role_patterns: vec![Regex::new("control-plane").unwrap()],
            ..host_settings()
        };
        let snapshot = snapshot_with_nodes(vec![
            Arc::new(node_with_roles("control-1", &["control-plane"])),
            Arc::new(node_with_roles("worker-1", &["worker"])),
        ]);

        let cluster = Cluster::new(&snapshot, &settings);
        assert_eq!(cluster.aggregation_nodes.len(), 1, "control plane excluded");

        let body = cluster
            .emit()
            .into_iter()
            .filter_map(Result::ok)
            .find_map(|section| match section.body {
                SectionBody::Json { name, body } if name == KubeNodeCountV1::NAME => Some(body),
                _ => None,
            })
            .expect("expected a kube_node_count_v1 section");
        let section: serde_json::Value = serde_json::from_str(&body).unwrap();

        let counted = section["nodes"].as_array().expect("nodes is an array");
        assert_eq!(counted.len(), 2, "both nodes are counted");
        assert_eq!(counted[0]["roles"][0], "control-plane");
        assert_eq!(counted[1]["roles"][0], "worker");
    }

    #[test]
    /// Test that [`Cluster::aggregation_pods()`] includes pods only running on
    /// non-excluded nodes.
    fn test_aggregation_pods() {
        let nodes = vec![Arc::new(node("worker-1"))];
        let aggregation_nodes: Vec<&Arc<Node>> = nodes.iter().collect();

        let pods = vec![
            Arc::new(pod("on-worker", Some("worker-1"))), // node is included
            Arc::new(pod("on-control", Some("control-plane"))), // node is dropped
            Arc::new(pod("unscheduled", None)),           // node is included
        ];

        let result = Cluster::aggregation_pods(&pods, &aggregation_nodes);
        let names: Vec<&str> = result
            .iter()
            .filter_map(|p| p.metadata.name.as_deref())
            .collect();
        assert_eq!(names, vec!["on-worker", "unscheduled"]);
    }
}
