use k8s_openapi::api::core::v1::{Node, Pod};
use std::collections::HashSet;
use std::ops::Add;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::PiggybackHost;
use crate::section::{
    performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1},
    resource::{KubeCpuResourcesV1, KubeMemoryResourcesV1, ResourceAccumulator, ResourceAxis},
    writeable::{SectionError, WriteableSection},
};
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
    /// argument and land in
    /// [`HostSettings.excluded_node_role_patterns`].
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

    fn cpu_resources(&self) -> KubeCpuResourcesV1 {
        let ra = self
            .aggregation_pods
            .iter()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Cpu))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeCpuResourcesV1(ra)
    }

    fn memory_resources(&self) -> KubeMemoryResourcesV1 {
        let ra = self
            .aggregation_pods
            .iter()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Memory))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeMemoryResourcesV1(ra)
    }

    fn cpu_performance(&self) -> Option<KubePerformanceCpuV1> {
        Some(KubePerformanceCpuV1::new(
            self.snapshot
                .metrics
                .aggregate(self.aggregation_pods.iter().copied())?
                .cpu_usage_nano_cores,
        ))
    }

    fn memory_performance(&self) -> Option<KubePerformanceMemoryV1> {
        Some(KubePerformanceMemoryV1::new(
            self.snapshot
                .metrics
                .aggregate(self.aggregation_pods.iter().copied())?
                .memory_working_set_bytes,
        ))
    }
}

impl PiggybackHost for Cluster<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = String::new();
        let mut out = vec![
            WriteableSection::of(me.clone(), &self.cpu_resources()),
            WriteableSection::of(me.clone(), &self.memory_resources()),
        ];
        if let Some(cpu_perf) = &self.cpu_performance() {
            out.push(WriteableSection::of(me.clone(), cpu_perf));
        }
        if let Some(mem_perf) = &self.memory_performance() {
            out.push(WriteableSection::of(me, mem_perf));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::PodSpec;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn pod(name: &str, node: Option<&str>) -> Arc<Pod> {
        Arc::new(Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                node_name: node.map(String::from),
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    fn node(name: &str) -> Arc<Node> {
        Arc::new(Node {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    #[test]
    /// Test that [`Cluster::aggregation_pods()`] includes pods only running on
    /// non-excluded nodes.
    fn test_aggregation_pods() {
        let nodes = vec![node("worker-1")];
        let aggregation_nodes: Vec<&Arc<Node>> = nodes.iter().collect();

        let pods = vec![
            pod("on-worker", Some("worker-1")),       // node is included
            pod("on-control", Some("control-plane")), // node is dropped
            pod("unscheduled", None),                 // node is included
        ];

        let result = Cluster::aggregation_pods(&pods, &aggregation_nodes);
        let names: Vec<&str> = result
            .iter()
            .filter_map(|p| p.metadata.name.as_deref())
            .collect();
        assert_eq!(names, vec!["on-worker", "unscheduled"]);
    }
}
