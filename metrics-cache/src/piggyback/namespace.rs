use k8s_openapi::api::core::v1;
use std::ops::Add;

use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    namespace::KubeNamespaceInfoV1,
    performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1},
    resource::{KubeCpuResourcesV1, KubeMemoryResourcesV1, ResourceAccumulator, ResourceAxis},
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct Namespace<'a> {
    api: &'a v1::Namespace,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
}

impl Namespace<'_> {
    pub fn new<'a>(api: &'a v1::Namespace, snapshot: &'a Snapshot) -> Option<Namespace<'a>> {
        Some(Namespace {
            api,
            meta: Meta::from_resource(api)?,
            snapshot,
        })
    }

    /// Generate the section `kube_namespace_info_v1` from a snapshot.
    fn info<'a>(&'a self) -> KubeNamespaceInfoV1<'a> {
        KubeNamespaceInfoV1 {
            name: self.meta.name,
            creation_timestamp: self
                .api
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: std::collections::BTreeMap::new(), // TODO
            annotations: std::collections::BTreeMap::new(), // TODO
            cluster: "TODO_CLUSTERNAME",               // TODO
            kubernetes_cluster_hostname: "TODO_CLUSTERNAME", // TODO
        }
    }

    fn cpu_resources(&self) -> KubeCpuResourcesV1 {
        let ra = self
            .snapshot
            .owner_graph
            .pods_by_namespace(self.meta.name)
            .iter()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Cpu))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeCpuResourcesV1(ra)
    }

    fn memory_resources(&self) -> KubeMemoryResourcesV1 {
        let ra = self
            .snapshot
            .owner_graph
            .pods_by_namespace(self.meta.name)
            .iter()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Memory))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeMemoryResourcesV1(ra)
    }

    fn cpu_performance(&self) -> Option<KubePerformanceCpuV1> {
        let pods = self.snapshot.owner_graph.pods_by_namespace(self.meta.name);
        Some(KubePerformanceCpuV1::new(
            self.snapshot.metrics.aggregate(pods)?.cpu_usage_nano_cores,
        ))
    }

    fn memory_performance(&self) -> Option<KubePerformanceMemoryV1> {
        let pods = self.snapshot.owner_graph.pods_by_namespace(self.meta.name);
        Some(KubePerformanceMemoryV1::new(
            self.snapshot
                .metrics
                .aggregate(pods)?
                .memory_working_set_bytes,
        ))
    }
}

impl PiggybackHost for Namespace<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname("TODO_CLUSTERNAME");
        let mut out = vec![
            WriteableSection::of(me.clone(), &self.info()),
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
