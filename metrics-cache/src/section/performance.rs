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

/// CPU performance/utilization. (`kube_performance_cpu_v1`)
#[derive(Serialize)]
pub(crate) struct KubePerformanceCpuV1 {
    resource: PerformanceFields,
}

impl KubePerformanceCpuV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self {
            resource: PerformanceFields {
                type_: PerformanceType::Cpu,
                usage: usage as f64 / 1e9,
            },
        }
    }
}

impl Section for KubePerformanceCpuV1 {
    const NAME: &'static str = "kube_performance_cpu_v1";
}

/// Memory performance/utilization. (`kube_performance_memory_v1`)
#[derive(Serialize)]
pub(crate) struct KubePerformanceMemoryV1 {
    resource: PerformanceFields,
}

impl KubePerformanceMemoryV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self {
            resource: PerformanceFields {
                type_: PerformanceType::Memory,
                usage: usage as f64,
            },
        }
    }
}

impl Section for KubePerformanceMemoryV1 {
    const NAME: &'static str = "kube_performance_memory_v1";
}

/// Swap performance/utilization. (`kube_performance_memory_swap_v1`)
#[derive(Serialize)]
pub(crate) struct KubePerformanceMemorySwapV1 {
    resource: PerformanceFields,
}

impl KubePerformanceMemorySwapV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self {
            resource: PerformanceFields {
                type_: PerformanceType::Memory,
                usage: usage as f64,
            },
        }
    }
}

impl Section for KubePerformanceMemorySwapV1 {
    const NAME: &'static str = "kube_performance_memory_swap_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kube_performance_cpu_v1() {
        insta::assert_json_snapshot!(KubePerformanceCpuV1::new(1_500_000_000));
    }

    #[test]
    fn kube_performance_memory_v1() {
        insta::assert_json_snapshot!(KubePerformanceMemoryV1::new(104_857_600));
    }

    #[test]
    fn kube_performance_memory_swap_v1() {
        insta::assert_json_snapshot!(KubePerformanceMemorySwapV1::new(114_962_432));
    }
}
