use k8s_openapi::api::core::v1::Pod;
use moka::future::Cache;
use std::collections::HashMap;
use std::ops::{Add, AddAssign};
use std::sync::Arc;

use crate::ingest::kubelet_stats::StatsSummary;

#[derive(Clone, Debug)]
pub struct Sample {
    pub cpu_usage_nano_cores: u64,
    pub memory_working_set_bytes: u64,
}

impl Add for Sample {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            cpu_usage_nano_cores: self.cpu_usage_nano_cores + rhs.cpu_usage_nano_cores,
            memory_working_set_bytes: self.memory_working_set_bytes + rhs.memory_working_set_bytes,
        }
    }
}

impl AddAssign for Sample {
    fn add_assign(&mut self, rhs: Self) {
        self.cpu_usage_nano_cores += rhs.cpu_usage_nano_cores;
        self.memory_working_set_bytes += rhs.memory_working_set_bytes;
    }
}

/// A slim extraction of the metrics we need from the [`StatsSummary`] for PVC
/// volumes.
#[derive(Debug)]
pub struct VolumeSample {
    pub available_bytes: u64,
    pub capacity_bytes: u64,
}

#[derive(Debug)]
pub struct MetricTables {
    /// Performance samples for containers.
    ///
    /// Indexed by: `sample = samples[namespace][pod][container]`
    pub containers: HashMap<String, HashMap<String, HashMap<String, Sample>>>,

    /// PVC volumes.
    ///
    /// We ignore volumes with no `pvcRef` (they aren't PVCs).
    ///
    /// Indexed by `volume = volumes[namespace][pvc_name]`
    pub pvc_volumes: HashMap<String, HashMap<String, VolumeSample>>,
}

impl MetricTables {
    /// Collect stats from the Kubelet stats summary and index them by some
    /// known key.
    ///
    /// We iterate each Kubelet stats summary (one per node) once, and iterate its
    /// pods once.
    pub fn from_cache(kubelet_stats_summary_cache: Cache<String, Arc<StatsSummary>>) -> Self {
        let mut containers: HashMap<String, HashMap<String, HashMap<String, Sample>>> =
            HashMap::new();
        let mut pvc_volumes: HashMap<String, HashMap<String, VolumeSample>> = HashMap::new();

        for (_, stats_summary) in kubelet_stats_summary_cache.iter() {
            for pod in &stats_summary.pods {
                // Containers
                let pod_map = containers
                    .entry(pod.pod_ref.namespace.clone())
                    .or_default()
                    .entry(pod.pod_ref.name.clone())
                    .or_default();
                for container in &pod.containers {
                    let sample = Sample {
                        cpu_usage_nano_cores: container
                            .cpu
                            .as_ref()
                            .and_then(|c| c.usage_nano_cores)
                            .unwrap_or(0),
                        memory_working_set_bytes: container
                            .memory
                            .as_ref()
                            .and_then(|m| m.working_set_bytes)
                            .unwrap_or(0),
                    };
                    pod_map.insert(container.name.clone(), sample);
                }

                // PVC volumes
                if let Some(volumes) = &pod.volume {
                    for volume in volumes {
                        if let Some(pvc_ref) = &volume.pvc_ref {
                            let Some(available_bytes) = volume.available_bytes else {
                                continue;
                            };
                            let Some(capacity_bytes) = volume.capacity_bytes else {
                                continue;
                            };
                            let sample = VolumeSample {
                                available_bytes,
                                capacity_bytes,
                            };
                            pvc_volumes
                                .entry(pvc_ref.namespace.clone())
                                .or_default()
                                .insert(pvc_ref.name.clone(), sample);
                        }
                    }
                }
            }
        }

        Self {
            containers,
            pvc_volumes,
        }
    }

    /// Given a namespace, pod, and container name, try to find the relevant
    /// metrics sample associated with the container. O(1).
    pub fn container(&self, namespace: &str, pod: &str, container: &str) -> Option<&Sample> {
        self.containers.get(namespace)?.get(pod)?.get(container)
    }

    /// Given a namespace and pod, try to find the roll-up (summed total) of
    /// all the containers in the pod. O(1) lookup and O(n) over the number of
    /// containers in the pod.
    pub fn pod_usage(&self, namespace: &str, pod: &str) -> Option<Sample> {
        let mut total = Sample {
            cpu_usage_nano_cores: 0,
            memory_working_set_bytes: 0,
        };
        for sample in self.containers.get(namespace)?.get(pod)?.values() {
            total.cpu_usage_nano_cores += sample.cpu_usage_nano_cores;
            total.memory_working_set_bytes += sample.memory_working_set_bytes;
        }
        Some(total)
    }

    /// Aggregate the samples for all of the given pods.
    ///
    /// If no sample for any pod is found, `None` is returned.
    /// Otherwise, the summed aggregate if all available samples for the given
    /// pods is returned.
    pub fn aggregate<'a>(&self, pods: impl IntoIterator<Item = &'a Arc<Pod>>) -> Option<Sample> {
        let mut out: Option<Sample> = None;
        for pod in pods {
            if let Some(ns) = &pod.metadata.namespace
                && let Some(name) = &pod.metadata.name
                && let Some(sample) = self.pod_usage(ns, name)
            {
                out = match out {
                    Some(total) => Some(sample + total),
                    None => Some(sample),
                };
            }
        }
        out
    }

    /// Get the slim [`VolumeSample`] given a namespace and PVC claim name.
    pub fn pvc_volume(&self, namespace: &str, claim_name: &str) -> Option<&VolumeSample> {
        self.pvc_volumes.get(namespace)?.get(claim_name)
    }
}
