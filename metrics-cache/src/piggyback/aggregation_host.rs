use k8s_openapi::api::core::v1;
use std::ops::Add;
use std::sync::Arc;

use crate::section::performance::{
    KubePerformanceCpuV1, KubePerformanceMemorySwapV1, KubePerformanceMemoryV1,
};
use crate::section::resource::{
    KubeCpuResourcesV1, KubeMemoryResourcesV1, ResourceAccumulator, ResourceAxis,
};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

/// Many hosts contain aggregations ("roll-ups") of resource and performance
/// data. These are so common among the piggyback hosts, that we abstract it
/// out to a trait that the hosts can implement to get free default
/// implementations of the methods, provided they supply the pod-set to
/// aggregate over.
///
/// The performance methods filter to pods in `Running` phase (via
/// [`crate::snapshot::metric_tables::MetricTables::aggregate()`]).
///
/// Thus, every "aggregation host" can simply implement [`Self::pods()`] and
/// [`Self::snapshot()`] and get the rest of the roll-up logic for free.
pub(crate) trait AggregationHost {
    /// A simple accessor for the current [`Snapshot`].
    fn snapshot(&self) -> &Snapshot;

    /// The pod-set to aggregate over.
    fn pods(&self) -> impl Iterator<Item = &Arc<v1::Pod>>;

    fn cpu_resources(&self) -> KubeCpuResourcesV1 {
        let ra = self
            .pods()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Cpu))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeCpuResourcesV1(ra)
    }

    fn memory_resources(&self) -> KubeMemoryResourcesV1 {
        let ra = self
            .pods()
            .map(|p| ResourceAccumulator::from_pod(p, ResourceAxis::Memory))
            .fold(ResourceAccumulator::default(), Add::add);
        KubeMemoryResourcesV1(ra)
    }

    fn cpu_performance(&self) -> Option<KubePerformanceCpuV1> {
        Some(KubePerformanceCpuV1::new(
            self.snapshot()
                .metrics
                .aggregate(self.pods())
                .cpu_usage_nano_cores?,
        ))
    }

    fn memory_performance(&self) -> Option<KubePerformanceMemoryV1> {
        Some(KubePerformanceMemoryV1::new(
            self.snapshot()
                .metrics
                .aggregate(self.pods())
                .memory_working_set_bytes?,
        ))
    }

    fn swap_performance(&self) -> Option<KubePerformanceMemorySwapV1> {
        Some(KubePerformanceMemorySwapV1::new(
            self.snapshot()
                .metrics
                .aggregate(self.pods())
                .swap_usage_bytes?,
        ))
    }

    /// Emit all possible resources and performance sections for the pod set
    /// returned by [`Self::pods()`].
    fn aggregation_sections(&self, me: &str) -> Vec<Result<WriteableSection, SectionError>> {
        let mut out = vec![
            WriteableSection::of(me, &self.cpu_resources()),
            WriteableSection::of(me, &self.memory_resources()),
        ];
        if let Some(cpu_perf) = &self.cpu_performance() {
            out.push(WriteableSection::of(me, cpu_perf));
        }
        if let Some(mem_perf) = &self.memory_performance() {
            out.push(WriteableSection::of(me, mem_perf));
        }
        if let Some(swap_perf) = &self.swap_performance() {
            out.push(WriteableSection::of(me, swap_perf));
        }
        out
    }
}
