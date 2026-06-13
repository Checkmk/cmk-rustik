use serde::Serialize;

use crate::section::Section;

#[derive(Serialize)]
pub(crate) enum PerformanceType {
    #[serde(rename = "cpu")]
    Cpu,
    #[serde(rename = "memory")]
    Memory,
}

#[derive(Serialize)]
pub(crate) struct PerformanceFields {
    pub(crate) type_: PerformanceType,
    pub(crate) usage: f64,
}

#[derive(Serialize)]
pub(crate) struct KubePerformanceCpuV1 {
    #[serde(flatten)]
    pub(crate) performance: PerformanceFields,
}

impl Section for KubePerformanceCpuV1 {
    const NAME: &'static str = "kube_performance_cpu_v1";
}

#[derive(Serialize)]
pub(crate) struct KubePerformanceMemoryV1 {
    #[serde(flatten)]
    pub(crate) performance: PerformanceFields,
}

impl Section for KubePerformanceMemoryV1 {
    const NAME: &'static str = "kube_performance_memory_v1";
}
