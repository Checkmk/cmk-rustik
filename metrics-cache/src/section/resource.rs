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
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::ResourceRequirements;
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

    use crate::test_support::*;

    /// Build a [`ResourceRequirements`] from (key, quantity) pairs. An empty
    /// slice means the requests/limits are not specified at all.
    fn requirements(requests: &[(&str, &str)], limits: &[(&str, &str)]) -> ResourceRequirements {
        let mut out = ResourceRequirements::default();
        for (key, quantity) in requests {
            out.requests
                .get_or_insert_with(Default::default)
                .insert(s(key), Quantity(s(quantity)));
        }
        for (key, quantity) in limits {
            out.limits
                .get_or_insert_with(Default::default)
                .insert(s(key), Quantity(s(quantity)));
        }
        out
    }

    /// When a pod does not request resources, the default
    /// [`ResourceAccumulator`] is returned (all zeroes).
    #[test]
    fn resource_accumulator_from_pod_no_resources() {
        let pod = pod_prefilled("no-resources");
        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Cpu),
            ResourceAccumulator::default()
        );
        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Memory),
            ResourceAccumulator::default()
        );
    }

    /// A Pod with pod-level CPU and memory requests+limits defined. Each
    /// axis must only pick up its own key.
    #[test]
    fn resource_accumulator_pod_level_resources() {
        let mut pod = pod_prefilled("pod-level-resources");
        pod.spec.as_mut().unwrap().resources = Some(requirements(
            &[("cpu", "500m"), ("memory", "1Gi")],
            &[("cpu", "900m"), ("memory", "2Gi")],
        ));
        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Cpu),
            ResourceAccumulator {
                request: 0.5,
                limit: 0.9,
                count_pods_pod_level_request: 1,
                count_pods_pod_level_limit: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Memory),
            ResourceAccumulator {
                request: 1073741824.0,
                limit: 2147483648.0,
                count_pods_pod_level_request: 1,
                count_pods_pod_level_limit: 1,
                ..Default::default()
            }
        );
    }

    /// A pod with unspecified or specified-and-0 pod-level resources should
    /// fall through to the container quantities which get summed up. On the CPU
    /// axis the same container counts as unspecified (sums stay zero, but
    /// the container is still counted).
    ///
    /// Explicitly, both cases are tested:
    /// - Pod-level resources are undefined
    /// - Pod-level resources defined and parsed to 0 (treated as unlimited)
    #[test]
    fn resource_accumulator_container_level_memory_resources() {
        fn assert_fallthrough_to_container(pod: &Pod) {
            assert_eq!(
                ResourceAccumulator::from_pod(pod, ResourceAxis::Memory),
                ResourceAccumulator {
                    request: 1073741824.0,
                    limit: 2147483648.0,
                    count_total_requests: 1,
                    count_total_limits: 1,
                    ..Default::default()
                }
            );
            assert_eq!(
                ResourceAccumulator::from_pod(pod, ResourceAxis::Cpu),
                ResourceAccumulator {
                    count_unspecified_requests: 1,
                    count_total_requests: 1,
                    count_unspecified_limits: 1,
                    count_total_limits: 1,
                    ..Default::default()
                }
            );
        }

        let mut pod = pod_prefilled("some-name");
        let mut container = container("test");
        container.resources = Some(requirements(&[("memory", "1Gi")], &[("memory", "2Gi")]));
        pod.spec.as_mut().unwrap().containers = vec![container];

        // When only container-level resources are specified, use them.
        assert_fallthrough_to_container(&pod);

        // 0 is unlimited: if pod-level are specified but all 0, still use container.
        pod.spec.as_mut().unwrap().resources = Some(requirements(
            &[("memory", "0Gi"), ("cpu", "0")],
            &[("memory", "0Gi"), ("cpu", "0")],
        ));
        assert_fallthrough_to_container(&pod);
    }

    /// Request/limit asymmetry: If a pod has requests and a container has both,
    /// requests and limits, then requests should come from the pod (container's
    /// requests ignored) and limits should come from the container.
    #[test]
    fn resource_accumulator_request_limit_asymmetry() {
        // Pod has requests
        let mut pod = pod_prefilled("the-pod");
        pod.spec.as_mut().unwrap().resources = Some(requirements(&[("memory", "70Mi")], &[]));

        // Container has both
        let mut container = container("test");
        container.resources = Some(requirements(&[("memory", "90Mi")], &[("memory", "600Mi")]));
        pod.spec.as_mut().unwrap().containers = vec![container];

        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Memory),
            ResourceAccumulator {
                request: parse_quantity("70Mi").unwrap(), // pod-level
                limit: parse_quantity("600Mi").unwrap(),  // container-level
                count_pods_pod_level_request: 1,
                count_total_limits: 1,
                ..Default::default()
            }
        );
    }

    /// Container-level 0 vs unspecified limit handling.
    #[test]
    fn resource_accumulator_container_0_vs_unspecified_limit() {
        let mut pod = pod_prefilled("cool-pod");

        let mut container_zero = container("zero");
        container_zero.resources = Some(requirements(&[], &[("memory", "0")]));

        let container_unspecified = container("unspecified");

        pod.spec.as_mut().unwrap().containers = vec![container_zero, container_unspecified];

        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Memory),
            ResourceAccumulator {
                count_zeroed_limits: 1,
                count_unspecified_limits: 1,
                count_unspecified_requests: 2,
                count_total_requests: 2,
                count_total_limits: 2,
                ..Default::default()
            }
        );
    }

    /// Summing across containers
    #[test]
    fn resource_accumulator_container_sum() {
        let mut pod = pod_prefilled("uncool-pod");

        let mut container1 = container("container1");
        let container2 = container("container2");
        let mut container3 = container("container3");
        container1.resources = Some(requirements(&[("cpu", "1000m")], &[]));
        container3.resources = Some(requirements(&[("cpu", "4m")], &[]));

        pod.spec.as_mut().unwrap().containers = vec![container1, container2, container3];

        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Cpu),
            ResourceAccumulator {
                request: parse_quantity("1000m").unwrap() + parse_quantity("4m").unwrap(),
                count_unspecified_limits: 3,
                count_unspecified_requests: 1,
                count_total_requests: 3,
                count_total_limits: 3,
                ..Default::default()
            }
        );
    }

    /// Rounding of quantities
    #[test]
    fn resource_accumulator_rounding() {
        let mut pod = pod_prefilled("really-evil-pod");
        pod.spec.as_mut().unwrap().resources =
            Some(requirements(&[("memory", "100.5"), ("cpu", "0.5m")], &[]));
        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Memory),
            ResourceAccumulator {
                request: 101.0,
                count_pods_pod_level_request: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            ResourceAccumulator::from_pod(&pod, ResourceAxis::Cpu),
            ResourceAccumulator {
                request: 0.001,
                count_pods_pod_level_request: 1,
                ..Default::default()
            }
        );
    }

    /// Adding of `ResourceAccumulators`
    #[test]
    fn resource_accumulator_add() {
        let ra1 = ResourceAccumulator {
            request: 12.0,
            limit: 42.42,
            count_unspecified_requests: 11,
            count_total_requests: 31,
            count_pods_pod_level_request: 137,
            count_unspecified_limits: 39,
            count_zeroed_limits: 481,
            count_total_limits: 4,
            count_pods_pod_level_limit: 26,
        };

        let ra2 = ResourceAccumulator {
            request: 94.0,
            limit: 403.2,
            count_unspecified_requests: 19,
            count_total_requests: 18,
            count_pods_pod_level_request: 11,
            count_unspecified_limits: 33,
            count_zeroed_limits: 4,
            count_total_limits: 2,
            count_pods_pod_level_limit: 9,
        };

        let expected = ResourceAccumulator {
            request: 106.0,
            limit: 445.62,
            count_unspecified_requests: 30,
            count_total_requests: 49,
            count_pods_pod_level_request: 148,
            count_unspecified_limits: 72,
            count_zeroed_limits: 485,
            count_total_limits: 6,
            count_pods_pod_level_limit: 35,
        };

        assert_eq!(ra1 + ra2, expected);
    }
}
