use k8s_openapi::api::core::v1::{Node, Pod};
use std::collections::HashSet;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, PiggybackHost};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

#[allow(dead_code)]
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
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = "";
        let mut out = Vec::new();
        out.extend(self.aggregation_sections(me));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

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
