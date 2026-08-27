use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;
use tracing::warn;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::namespace::KubeNamespaceInfoV1;
use crate::section::resource_quota::criteria::ResourceQuotaCriteria;
use crate::section::resource_quota::{
    KubeResourceQuotaCpuResourcesV1, KubeResourceQuotaMemoryResourcesV1,
    KubeResourceQuotaPerformanceCpuV1, KubeResourceQuotaPerformanceMemoryV1,
};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;
use crate::snapshot::metric_tables::{MetricTables, Sample};

pub struct Namespace<'a> {
    api: &'a v1::Namespace,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
}

impl Namespace<'_> {
    pub fn new<'a>(
        api: &'a v1::Namespace,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<Namespace<'a>> {
        // To match Python: Only create a namespace host if it has a running or pending pod
        let meta = Meta::from_resource(api)?;
        let has_active_pod = snapshot
            .indexes
            .pods_by_namespace(meta.name)
            .iter()
            .any(|p| {
                matches!(
                    p.status.as_ref().and_then(|s| s.phase.as_deref()),
                    Some("Running" | "Pending")
                )
            });
        if has_active_pod {
            Some(Namespace {
                api,
                meta,
                snapshot,
                settings,
            })
        } else {
            None
        }
    }

    /// Aggregate kubelet metrics for running pods matched by this namespace's
    /// indexed ResourceQuota. Unsupported criteria are logged and return
    /// `None`.
    fn resource_quota_usage(&self) -> Option<Sample> {
        let quota = self.snapshot.indexes.resource_quota(self.meta.name)?;
        quota_usage(quota, self.pods(), &self.snapshot.metrics)
    }
}

fn quota_usage<'a>(
    quota: &v1::ResourceQuota,
    pods: impl Iterator<Item = &'a Arc<v1::Pod>>,
    metrics: &MetricTables,
) -> Option<Sample> {
    let criteria = match ResourceQuotaCriteria::try_from(quota) {
        Ok(criteria) => criteria,
        Err(err) => {
            let quota_name = quota.metadata.name.as_deref().unwrap_or("<unknown>");
            warn!(
                namespace = ?quota.metadata.namespace,
                quota = quota_name,
                %err,
                "skipping resource quota performance sections",
            );
            return None;
        }
    };
    let pods_matching_criteria = pods.filter(|pod| criteria.matches(pod));
    Some(metrics.aggregate(pods_matching_criteria))
}

impl AggregationHost for Namespace<'_> {
    fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    fn pods(&self) -> impl Iterator<Item = &Arc<v1::Pod>> {
        self.snapshot
            .indexes
            .pods_by_namespace(self.meta.name)
            .iter()
    }
}

impl PiggybackHost for Namespace<'_> {
    fn metadata(&self) -> Option<&ObjectMeta> {
        Some(&self.api.metadata)
    }

    fn kind(&self) -> &str {
        &self.meta.kind
    }

    fn namespace_for_filtering(&self) -> Option<&str> {
        Some(self.meta.name)
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();

        if let Some(kube_namespace_info_v1) =
            KubeNamespaceInfoV1::from_namespace(self.api, self.settings)
        {
            out.push(WriteableSection::of(&me, &kube_namespace_info_v1));
        }

        if let Some(kube_resource_quota_memory_resources_v1) =
            KubeResourceQuotaMemoryResourcesV1::from_namespace(self.api, &self.snapshot.indexes)
        {
            out.push(WriteableSection::of(
                &me,
                &kube_resource_quota_memory_resources_v1,
            ));
        }

        if let Some(kube_resource_quota_cpu_resources_v1) =
            KubeResourceQuotaCpuResourcesV1::from_namespace(self.api, &self.snapshot.indexes)
        {
            out.push(WriteableSection::of(
                &me,
                &kube_resource_quota_cpu_resources_v1,
            ));
        }

        // ResourceQuota performance
        if let Some(resource_quota_sample) = self.resource_quota_usage() {
            if let Some(cpu) = resource_quota_sample.cpu_usage_nano_cores {
                let kube_resource_quota_performance_cpu_v1 =
                    KubeResourceQuotaPerformanceCpuV1::new(cpu);
                out.push(WriteableSection::of(
                    &me,
                    &kube_resource_quota_performance_cpu_v1,
                ));
            }

            if let Some(memory) = resource_quota_sample.memory_working_set_bytes {
                let kube_resource_quota_performance_memory_v1 =
                    KubeResourceQuotaPerformanceMemoryV1::new(memory);
                out.push(WriteableSection::of(
                    &me,
                    &kube_resource_quota_performance_memory_v1,
                ));
            }
        }

        out.extend(self.aggregation_sections(&me));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use k8s_openapi::api::core::v1::PodStatus;
    use std::collections::HashMap;

    use crate::test_support::{pod, s};

    const NAMESPACE: &str = "my-ns";

    /// A running pod in [`NAMESPACE`], optionally with a priority class.
    fn running_pod(name: &str, priority_class: Option<&str>) -> Arc<v1::Pod> {
        let mut pod = pod(name, Some("node"));
        pod.metadata.namespace = Some(s(NAMESPACE));
        pod.spec
            .as_mut()
            .expect("fixture pod should have a spec")
            .priority_class_name = priority_class.map(s);
        pod.status = Some(PodStatus {
            phase: Some(s("Running")),
            ..Default::default()
        });
        Arc::new(pod)
    }

    fn quota_with_scopes(scopes: &[&str]) -> v1::ResourceQuota {
        v1::ResourceQuota {
            spec: Some(v1::ResourceQuotaSpec {
                scopes: Some(scopes.iter().copied().map(s).collect()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Usage for each named pod in [`NAMESPACE`], as a single container.
    fn metrics_for(pods: &[(&str, Sample)]) -> MetricTables {
        let mut metrics = MetricTables::default();
        let namespace_metrics = metrics.containers.entry(s(NAMESPACE)).or_default();
        for (pod_name, sample) in pods {
            namespace_metrics.insert(s(pod_name), HashMap::from([(s("container"), *sample)]));
        }
        metrics
    }

    /// Only pods satisfying the quota's scopes contribute to the aggregate. The
    /// two pods differ on both axes, so an unfiltered sum cannot coincidentally
    /// equal the matching pod's usage.
    #[test]
    fn quota_usage_only_aggregates_matching_pods() {
        let pods = [
            running_pod("matching", Some("high")),
            running_pod("other", None),
        ];
        let matching = Sample {
            cpu_usage_nano_cores: Some(1_000_000_000),
            memory_working_set_bytes: Some(100),
            swap_usage_bytes: None,
        };
        let unmatched = Sample {
            cpu_usage_nano_cores: Some(2_000_000_000),
            memory_working_set_bytes: Some(200),
            swap_usage_bytes: None,
        };
        let metrics = metrics_for(&[("matching", matching), ("other", unmatched)]);

        let usage = quota_usage(
            &quota_with_scopes(&["PriorityClass"]),
            pods.iter(),
            &metrics,
        );

        assert_eq!(usage, Some(matching));
    }

    /// A scope we cannot evaluate must yield no usage at all, rather than an
    /// aggregate over every pod.
    #[test]
    fn quota_usage_skips_unsupported_scopes() {
        let pods = [running_pod("any", None)];
        let metrics = metrics_for(&[(
            "any",
            Sample {
                cpu_usage_nano_cores: Some(1_000_000_000),
                memory_working_set_bytes: Some(100),
                swap_usage_bytes: None,
            },
        )]);

        let usage = quota_usage(
            &quota_with_scopes(&["CrossNamespacePodAffinity"]),
            pods.iter(),
            &metrics,
        );

        assert!(usage.is_none());
    }
}
