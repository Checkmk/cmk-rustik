use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::{
    common::LabelRef,
    namespace::KubeNamespaceInfoV1,
    writeable::{SectionError, WriteableSection},
};
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

    /// Generate the section `kube_namespace_info_v1` from a snapshot.
    fn info<'a>(&'a self) -> KubeNamespaceInfoV1<'a> {
        KubeNamespaceInfoV1 {
            name: self.meta.name,
            creation_timestamp: self
                .api
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: self
                .api
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: self
                .api
                .metadata
                .annotations
                .as_ref()
                .map(|m| self.settings.annotation_key_pattern.filter(m))
                .unwrap_or_default(),
            cluster: &self.settings.cluster_name,
            kubernetes_cluster_hostname: &self.settings.cluster_host_name,
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
        let mut out = vec![WriteableSection::of(&me, &self.info())];
        out.extend(self.aggregation_sections(&me));
        out
    }
}
