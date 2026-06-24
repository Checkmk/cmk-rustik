use k8s_openapi::api::core::v1::Pod;
use serde::Serialize;
use std::ops::Add;

use crate::section::Section;
use crate::section::common::parse_quantity;

#[derive(Debug)]
pub enum ResourceAxis {
    Cpu,
    Memory,
}

/// Aggregated resource requests and limits for a single axis (CPU or memory),
/// summed over some set of pods.
///
/// This type is used as both the roll-up accumulator and the section payload
/// for `kube_cpu_resources_v1` and `kube_memory_resources_v1`. One pod produces
/// one of these via [`Self::from_pod()`]. Larger scopes (controller, namespace,
/// cluster) are built by summing their per-pod values. It forms a monoid under
/// field-wise addition and the [`Default`] is the zero accumulator.
///
/// `request` and `limit` are sums in the axis's section unit (cores for CPU,
/// bytes for memory). The `count_*` fields are bookkeeping so that the check
/// plugin can report how many containers were counted, how many left a limit or
/// request unspecified, how many limits were zero (unlimited), and how many
/// pods declared pod-level resources.
#[derive(Copy, Clone, Debug, Default, Serialize)]
pub struct ResourceAccumulator {
    /// The sum of requested units (cores for CPU, bytes for memory)
    request: f64,
    /// The sum of specified limits (cores for CPU, bytes for memory).
    limit: f64,
    /// The number of containers that did not specify any requests.
    /// Pods with pod-level requests set do not contribute containers to this.
    count_unspecified_requests: u32,
    /// The total number of containers *considered* for requests. It includes
    /// containers that might not *actually* have requests specified.
    /// Pods with pod-level requests set do not contribute containers to this.
    count_total_requests: u32,
    /// The number of pods aggregated that specified pod-level requests.
    /// Pods with pod-level requests set _do_ contribute to this count.
    count_pods_pod_level_request: u32,
    /// The number of containers that did not specify any limits.
    /// Pods with pod-level limits set do not contribute containers to this.
    count_unspecified_limits: u32,
    /// The number of containers that specified limits as 0.
    /// Pods with pod-level limits set do not contribute containers to this.
    count_zeroed_limits: u32,
    /// The total number of containers *considered* for limits. It includes
    /// containers that might not *actually* have limits specified.
    /// Pods with pod-level limits set do not contribute containers to this.
    count_total_limits: u32,
    /// The number of pods aggregated that specified pod-level limits.
    /// Pods with pod-level limits set _do_ contribute to this count.
    count_pods_pod_level_limit: u32,
}

impl ResourceAccumulator {
    /// Given a pod, create a single `ResourceAccumulator`.
    ///
    /// Importantly, we handle both container-level and pod-level resources
    /// here. If pod-level requests are set, we use those (only) in the
    /// resulting instance. Otherwise the sum of the requests of all the
    /// containers in the pod is used. Similarly for limits.
    pub fn from_pod(pod: &Pod, axis: ResourceAxis) -> Self {
        let (key, round): (&str, fn(f64) -> f64) = match axis {
            ResourceAxis::Cpu => ("cpu", |v| (v * 1000.0).ceil() / 1000.0),
            ResourceAxis::Memory => ("memory", f64::ceil),
        };
        let mut ra: ResourceAccumulator = Default::default();
        let Some(ref spec) = pod.spec else {
            // If there's somehow no spec, just return the zeroed accumulator.
            return ra;
        };

        // Requests
        if let Some(request) = spec
            .resources
            .as_ref()
            .and_then(|r| r.requests.as_ref())
            .and_then(|r| r.get(key))
            .and_then(|q| parse_quantity(&q.0))
            .map(round)
            && request > 0.0
        {
            // The pod has specified pod-level requests > 0 (0 is unlimited).
            // In this case, we *only* count the pod-level requests.
            ra.request += request;
            ra.count_pods_pod_level_request += 1;
        } else {
            // No pod requests, but maybe some of the containers have requests.
            for container in &spec.containers {
                // count_total_requests includes *all* considered containers,
                // even with unspecified requests.
                ra.count_total_requests += 1;
                if let Some(request) = container
                    .resources
                    .as_ref()
                    .and_then(|r| r.requests.as_ref())
                    .and_then(|r| r.get(key))
                    .and_then(|q| parse_quantity(&q.0))
                    .map(round)
                {
                    ra.request += request;
                } else {
                    ra.count_unspecified_requests += 1;
                }
            }
        }

        // Limits
        if let Some(limit) = spec
            .resources
            .as_ref()
            .and_then(|r| r.limits.as_ref())
            .and_then(|r| r.get(key))
            .and_then(|q| parse_quantity(&q.0))
            .map(round)
            && limit > 0.0
        {
            // The pod has specified pod-level limits > 0 (0 is unlimited).
            // In this case, we *only* count the pod-level limits.
            ra.limit += limit;
            ra.count_pods_pod_level_limit += 1;
        } else {
            // No pod limits, but maybe some of the containers have limits.
            for container in &spec.containers {
                ra.count_total_limits += 1;
                if let Some(limit) = container
                    .resources
                    .as_ref()
                    .and_then(|r| r.limits.as_ref())
                    .and_then(|r| r.get(key))
                    .and_then(|q| parse_quantity(&q.0))
                    .map(round)
                {
                    ra.limit += limit;
                    if limit == 0.0 {
                        ra.count_zeroed_limits += 1
                    }
                } else {
                    ra.count_unspecified_limits += 1
                }
            }
        }

        ra
    }
}

impl Add for ResourceAccumulator {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            request: self.request + rhs.request,
            limit: self.limit + rhs.limit,
            count_unspecified_requests: self.count_unspecified_requests
                + rhs.count_unspecified_requests,
            count_total_requests: self.count_total_requests + rhs.count_total_requests,
            count_pods_pod_level_request: self.count_pods_pod_level_request
                + rhs.count_pods_pod_level_request,
            count_unspecified_limits: self.count_unspecified_limits + rhs.count_unspecified_limits,
            count_zeroed_limits: self.count_zeroed_limits + rhs.count_zeroed_limits,
            count_total_limits: self.count_total_limits + rhs.count_total_limits,
            count_pods_pod_level_limit: self.count_pods_pod_level_limit
                + rhs.count_pods_pod_level_limit,
        }
    }
}

/// Memory resources. (`kube_memory_resources_v1`)
#[derive(Debug, Serialize)]
pub struct KubeMemoryResourcesV1(pub ResourceAccumulator);

impl Section for KubeMemoryResourcesV1 {
    const NAME: &'static str = "kube_memory_resources_v1";
}

/// CPU resources. (`kube_cpu_resources_v1`)
#[derive(Debug, Serialize)]
pub struct KubeCpuResourcesV1(pub ResourceAccumulator);

impl Section for KubeCpuResourcesV1 {
    const NAME: &'static str = "kube_cpu_resources_v1";
}
