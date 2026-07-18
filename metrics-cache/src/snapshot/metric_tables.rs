use k8s_openapi::api::core::v1::Pod;
use moka::future::Cache;
use std::collections::HashMap;
use std::ops::{Add, AddAssign};
use std::sync::Arc;

use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::kubelet_stats::StatsSummary;

/// One point-in-time performance sample for a container (or a sum of such
/// samples).
///
/// A field is `None` when the kubelet did not report that metric. This is a
/// normal, transient state: `usageNanoCores` is a rate and needs two scrapes
/// to compute, so a just-started container reports memory but no CPU.
///
/// A missing sample means *absent*, never zero: absence must not turn into
/// fake zero datapoints downstream.
///
/// `Sample::default()` (all fields `None`) is the identity for `+`/`+=`,
/// which makes it the correct seed for accumulation loops.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Sample {
    pub cpu_usage_nano_cores: Option<u64>,
    pub memory_working_set_bytes: Option<u64>,
}

impl Add for Sample {
    type Output = Self;
    fn add(mut self, rhs: Self) -> Self {
        self += rhs;
        self
    }
}

/// Field-wise accumulation: a `None` on either side contributes nothing
/// and never wipes out an existing sum; the first `Some` a field sees
/// flips the accumulator's field to `Some`.
impl AddAssign for Sample {
    fn add_assign(&mut self, rhs: Self) {
        match (&mut self.cpu_usage_nano_cores, rhs.cpu_usage_nano_cores) {
            (Some(a), Some(b)) => *a += b,
            (a @ None, b) => *a = b,
            _ => {}
        }
        match (
            &mut self.memory_working_set_bytes,
            rhs.memory_working_set_bytes,
        ) {
            (Some(a), Some(b)) => *a += b,
            (a @ None, b) => *a = b,
            _ => {}
        }
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
    pub fn from_cache(
        kubelet_stats_summary_cache: &Cache<String, Arc<MetricsFetcherIngestion<StatsSummary>>>,
    ) -> Self {
        let mut containers: HashMap<String, HashMap<String, HashMap<String, Sample>>> =
            HashMap::new();
        let mut pvc_volumes: HashMap<String, HashMap<String, VolumeSample>> = HashMap::new();

        for (_, stats_summary) in kubelet_stats_summary_cache.iter() {
            for pod in &stats_summary.payload.pods {
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
                            .and_then(|c| c.usage_nano_cores),
                        memory_working_set_bytes: container
                            .memory
                            .as_ref()
                            .and_then(|m| m.working_set_bytes),
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
    ///
    /// `None` means the pod is unknown to the table. A known pod whose
    /// containers have reported nothing yields `Some(Sample::default())`.
    pub fn pod_usage(&self, namespace: &str, pod: &str) -> Option<Sample> {
        let mut total = Sample::default();
        for sample in self.containers.get(namespace)?.get(pod)?.values() {
            total += *sample;
        }
        Some(total)
    }

    /// Aggregate the samples for all of the given pods.
    ///
    /// Sums the available samples of the given pods **in `Running` phase**
    /// (pods in other phases are ignored). The sum over no pods, or no
    /// samples, is `Sample::default()`; each field stays `None` until some
    /// pod contributes to it.
    ///
    /// A sum covers the pods that reported the metric: a pod without a
    /// sample (typically just-started) contributes nothing rather than
    /// suppressing the aggregate.
    pub fn aggregate<'a>(&self, pods: impl IntoIterator<Item = &'a Arc<Pod>>) -> Sample {
        let mut out: Sample = Sample::default();
        for pod in pods {
            if pod.status.as_ref().and_then(|s| s.phase.as_deref()) != Some("Running") {
                continue;
            }

            if let Some(ns) = &pod.metadata.namespace
                && let Some(name) = &pod.metadata.name
                && let Some(sample) = self.pod_usage(ns, name)
            {
                out += sample;
            }
        }
        out
    }

    /// Get the slim [`VolumeSample`] given a namespace and PVC claim name.
    pub fn pvc_volume(&self, namespace: &str, claim_name: &str) -> Option<&VolumeSample> {
        self.pvc_volumes.get(namespace)?.get(claim_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu_usage_nano_cores: Option<u64>, memory_working_set_bytes: Option<u64>) -> Sample {
        Sample {
            cpu_usage_nano_cores,
            memory_working_set_bytes,
        }
    }

    #[test]
    fn add_assign() {
        for ((l1, l2), (r1, r2), (expected1, expected2)) in [
            ((None, None), (None, None), (None, None)),
            ((Some(3), None), (None, None), (Some(3), None)),
            ((None, Some(4)), (None, Some(4)), (None, Some(8))),
            ((Some(1), Some(4)), (Some(2), Some(4)), (Some(3), Some(8))),
            ((None, None), (Some(2), Some(4)), (Some(2), Some(4))),
            ((Some(2), Some(4)), (None, None), (Some(2), Some(4))),
        ] {
            let mut acc = sample(l1, l2);
            acc += sample(r1, r2);
            assert_eq!(acc, sample(expected1, expected2));
        }
    }
}
