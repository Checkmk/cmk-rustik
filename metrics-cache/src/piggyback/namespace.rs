use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::namespace::KubeNamespaceInfoV1;
use crate::section::resource_quota::{
    KubeResourceQuotaCpuResourcesV1, KubeResourceQuotaMemoryResourcesV1,
};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

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

        out.extend(self.aggregation_sections(&me));
        out
    }
}
