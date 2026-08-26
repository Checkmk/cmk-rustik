pub(crate) mod criteria;

use k8s_openapi::api::core::v1::{Namespace, ResourceQuota};
use serde::Serialize;

use crate::section::Section;
use crate::section::common::parse_quantity;
use crate::section::performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1};
use crate::section::resource::ResourceAxis;
use crate::snapshot::indexes::Indexes;

#[derive(Serialize)]
struct ResourceQuotaSection {
    pub limit: Option<f64>,
    pub request: Option<f64>,
}

impl ResourceQuotaSection {
    fn from_resource_quota(
        quota: &ResourceQuota,
        axis: ResourceAxis,
    ) -> Option<ResourceQuotaSection> {
        let hard = quota.spec.as_ref()?.hard.as_ref()?;
        let axis_key = axis.key();

        let limit = hard
            .get(&format!("limits.{axis_key}"))
            .and_then(|q| parse_quantity(&q.0))
            .map(|value| axis.round_quantity(value));

        let request = hard
            .get(&format!("requests.{axis_key}"))
            .or_else(|| hard.get(axis_key))
            .and_then(|q| parse_quantity(&q.0))
            .map(|value| axis.round_quantity(value));

        match (limit, request) {
            (None, None) => None,
            _ => Some(Self { limit, request }),
        }
    }
}

/// Resource Quota memory resources. (`kube_resource_quota_memory_resources_v1`)
#[derive(Serialize)]
pub(crate) struct KubeResourceQuotaMemoryResourcesV1 {
    #[serde(flatten)]
    body: ResourceQuotaSection,
}

impl KubeResourceQuotaMemoryResourcesV1 {
    pub fn from_namespace(namespace: &Namespace, indexes: &Indexes) -> Option<Self> {
        let namespace_name = namespace.metadata.name.as_deref()?;
        let quota = indexes.resource_quota(namespace_name)?;
        Some(Self {
            body: ResourceQuotaSection::from_resource_quota(quota, ResourceAxis::Memory)?,
        })
    }
}

impl Section for KubeResourceQuotaMemoryResourcesV1 {
    const NAME: &'static str = "kube_resource_quota_memory_resources_v1";
}

/// Resource Quota CPU resources. (`kube_resource_quota_cpu_resources_v1`)
#[derive(Serialize)]
pub(crate) struct KubeResourceQuotaCpuResourcesV1 {
    #[serde(flatten)]
    body: ResourceQuotaSection,
}

impl KubeResourceQuotaCpuResourcesV1 {
    pub fn from_namespace(namespace: &Namespace, indexes: &Indexes) -> Option<Self> {
        let namespace_name = namespace.metadata.name.as_deref()?;
        let quota = indexes.resource_quota(namespace_name)?;
        Some(Self {
            body: ResourceQuotaSection::from_resource_quota(quota, ResourceAxis::Cpu)?,
        })
    }
}

impl Section for KubeResourceQuotaCpuResourcesV1 {
    const NAME: &'static str = "kube_resource_quota_cpu_resources_v1";
}

/// Actual CPU usage of the running pods matched by a ResourceQuota.
/// (`kube_resource_quota_performance_cpu_v1`)
#[derive(Serialize)]
pub(crate) struct KubeResourceQuotaPerformanceCpuV1(KubePerformanceCpuV1);

impl KubeResourceQuotaPerformanceCpuV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self(KubePerformanceCpuV1::new(usage))
    }
}

impl Section for KubeResourceQuotaPerformanceCpuV1 {
    const NAME: &'static str = "kube_resource_quota_performance_cpu_v1";
}

/// Actual memory usage of the running pods matched by a ResourceQuota.
/// (`kube_resource_quota_performance_memory_v1`)
#[derive(Serialize)]
pub(crate) struct KubeResourceQuotaPerformanceMemoryV1(KubePerformanceMemoryV1);

impl KubeResourceQuotaPerformanceMemoryV1 {
    pub(crate) fn new(usage: u64) -> Self {
        Self(KubePerformanceMemoryV1::new(usage))
    }
}

impl Section for KubeResourceQuotaPerformanceMemoryV1 {
    const NAME: &'static str = "kube_resource_quota_performance_memory_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use k8s_openapi::api::core::v1::ResourceQuotaSpec;
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Arc;

    use crate::test_support::*;

    fn resource_quota_hard(
        cpu_request: Option<&str>,
        cpu_limit: Option<&str>,
        mem_request: Option<&str>,
        mem_limit: Option<&str>,
        namespace: &str,
    ) -> ResourceQuota {
        let mut map = BTreeMap::new();
        if let Some(cpu_request) = cpu_request {
            map.insert(s("requests.cpu"), Quantity(cpu_request.to_string()));
        }
        if let Some(cpu_limit) = cpu_limit {
            map.insert(s("limits.cpu"), Quantity(cpu_limit.to_string()));
        }

        if let Some(mem_request) = mem_request {
            map.insert(s("requests.memory"), Quantity(mem_request.to_string()));
        }

        if let Some(mem_limit) = mem_limit {
            map.insert(s("limits.memory"), Quantity(mem_limit.to_string()));
        }

        ResourceQuota {
            metadata: ObjectMeta {
                name: Some(s("my-quota")),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            spec: Some(ResourceQuotaSpec {
                hard: Some(map),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn to_indexes(quota: ResourceQuota, namespace: &str) -> Indexes {
        Indexes {
            resource_quota_by_namespace: HashMap::from([(namespace.to_string(), Arc::new(quota))]),
            ..Default::default()
        }
    }

    #[test]
    fn kube_resource_quota_memory_resources_v1() {
        let namespace = namespace("my-ns");
        let quota = resource_quota_hard(
            Some("500m"),
            Some("1000m"),
            Some("520Mi"),
            Some("6Gi"),
            "my-ns",
        );
        let indexes = to_indexes(quota, "my-ns");
        insta::assert_json_snapshot!(KubeResourceQuotaMemoryResourcesV1::from_namespace(
            &namespace, &indexes
        ));
    }

    #[test]
    fn kube_resource_quota_cpu_resources_v1() {
        let namespace = namespace("my-ns");
        let quota = resource_quota_hard(
            Some("500m"),
            Some("1000m"),
            Some("520Mi"),
            Some("6Gi"),
            "my-ns",
        );
        let indexes = to_indexes(quota, "my-ns");
        insta::assert_json_snapshot!(KubeResourceQuotaCpuResourcesV1::from_namespace(
            &namespace, &indexes
        ));
    }

    #[test]
    fn resource_quota_cpu_rounding() {
        let namespace = namespace("my-ns");
        let quota = resource_quota_hard(Some("0.1m"), None, None, None, "my-ns");
        let indexes = to_indexes(quota, "my-ns");
        let section = KubeResourceQuotaCpuResourcesV1::from_namespace(&namespace, &indexes);
        assert_eq!(section.unwrap().body.request, Some(0.001));
    }

    #[test]
    fn resource_quota_memory_rounding() {
        let namespace = namespace("my-ns");
        let quota = resource_quota_hard(None, None, Some("1m"), None, "my-ns");
        let indexes = to_indexes(quota, "my-ns");
        let section = KubeResourceQuotaMemoryResourcesV1::from_namespace(&namespace, &indexes);
        assert_eq!(section.unwrap().body.request, Some(1.0));
    }

    #[test]
    fn resource_quota_none() {
        let namespace = namespace("my-ns");
        let quota = resource_quota_hard(None, None, None, None, "my-ns");
        let indexes = to_indexes(quota, "my-ns");
        let cpu = KubeResourceQuotaCpuResourcesV1::from_namespace(&namespace, &indexes);
        let mem = KubeResourceQuotaMemoryResourcesV1::from_namespace(&namespace, &indexes);
        assert!(cpu.is_none());
        assert!(mem.is_none());
    }

    #[test]
    fn bare_resource_keys_are_requests() {
        let namespace = namespace("my-ns");
        let mut quota = resource_quota_hard(None, None, None, None, "my-ns");
        let hard = quota
            .spec
            .as_mut()
            .and_then(|spec| spec.hard.as_mut())
            .expect("fixture should have hard requirements");
        hard.insert(s("cpu"), Quantity(s("500m")));
        hard.insert(s("memory"), Quantity(s("1Mi")));
        let indexes = to_indexes(quota, "my-ns");

        let cpu = KubeResourceQuotaCpuResourcesV1::from_namespace(&namespace, &indexes)
            .expect("bare cpu should produce a section");
        let memory = KubeResourceQuotaMemoryResourcesV1::from_namespace(&namespace, &indexes)
            .expect("bare memory should produce a section");
        assert_eq!((cpu.body.request, cpu.body.limit), (Some(0.5), None));
        assert_eq!(
            (memory.body.request, memory.body.limit),
            (Some(1048576.0), None)
        );
    }
}
