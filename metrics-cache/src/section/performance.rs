use serde::Serialize;

use crate::section::Section;

#[derive(Serialize)]
enum PerformanceType {
    #[serde(rename = "cpu")]
    Cpu,
    #[serde(rename = "memory")]
    Memory,
}

#[derive(Serialize)]
struct PerformanceFields {
    type_: PerformanceType,
    usage: f64,
}

#[derive(Serialize)]
pub(crate) struct KubePerformanceCpuV1 {
    #[serde(flatten)]
    performance: PerformanceFields,
}

impl KubePerformanceCpuV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self {
            performance: PerformanceFields {
                type_: PerformanceType::Cpu,
                usage: usage as f64 / 1e9,
            },
        }
    }
}

impl Section for KubePerformanceCpuV1 {
    const NAME: &'static str = "kube_performance_cpu_v1";
}

#[derive(Serialize)]
pub(crate) struct KubePerformanceMemoryV1 {
    #[serde(flatten)]
    performance: PerformanceFields,
}

impl KubePerformanceMemoryV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self {
            performance: PerformanceFields {
                type_: PerformanceType::Memory,
                usage: usage as f64,
            },
        }
    }
}

impl Section for KubePerformanceMemoryV1 {
    const NAME: &'static str = "kube_performance_memory_v1";
}
