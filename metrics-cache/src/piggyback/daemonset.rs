use k8s_openapi::api::apps::v1;
use std::ops::Add;

use crate::host_settings::HostSettings;
use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1},
    resource::{KubeCpuResourcesV1, KubeMemoryResourcesV1, ResourceAccumulator, ResourceAxis},
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct DaemonSet<'a> {
    _api: &'a v1::DaemonSet,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
    uid: &'a str,
}

impl DaemonSet<'_> {
    pub fn new<'a>(
        api: &'a v1::DaemonSet,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<DaemonSet<'a>> {
        let meta = Meta::from_resource(api)?;
        let uid = api.metadata.uid.as_deref()?;
        Some(DaemonSet {
            _api: api,
            meta,
            snapshot,
            settings,
            uid,
        })
    }

    fn cpu_resources(&self) -> KubeCpuResourcesV1 {
        let ra = self
            .snapshot
            .owner_graph
            .pods_by_controller(self.uid)
            .iter()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Cpu))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeCpuResourcesV1(ra)
    }

    fn memory_resources(&self) -> KubeMemoryResourcesV1 {
        let ra = self
            .snapshot
            .owner_graph
            .pods_by_controller(self.uid)
            .iter()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Memory))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeMemoryResourcesV1(ra)
    }

    fn cpu_performance(&self) -> Option<KubePerformanceCpuV1> {
        let pods = self.snapshot.owner_graph.pods_by_controller(self.uid);
        Some(KubePerformanceCpuV1::new(
            self.snapshot.metrics.aggregate(pods)?.cpu_usage_nano_cores,
        ))
    }

    fn memory_performance(&self) -> Option<KubePerformanceMemoryV1> {
        let pods = self.snapshot.owner_graph.pods_by_controller(self.uid);
        Some(KubePerformanceMemoryV1::new(
            self.snapshot
                .metrics
                .aggregate(pods)?
                .memory_working_set_bytes,
        ))
    }
}

impl PiggybackHost for DaemonSet<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
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
