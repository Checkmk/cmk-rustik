use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::pod::{
    KubePodConditionsV1, KubePodContainerSpecsV1, KubePodContainersV1, KubePodInfoV1,
    KubePodInitContainerSpecsV1, KubePodInitContainersV1, KubePodLifecycleV1, KubeStartTimeV1,
};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct Pod<'a> {
    api: &'a Arc<v1::Pod>,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
}

impl<'a> Pod<'a> {
    pub fn new(
        api: &'a Arc<v1::Pod>,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<Pod<'a>> {
        Some(Pod {
            api,
            meta: Meta::from_resource(api.as_ref())?,
            snapshot,
            settings,
        })
    }

    /// Generate the section `kube_pod_info_v1` from a snapshot.
    fn info(&'a self) -> Option<KubePodInfoV1<'a>> {
        let owner_graph = &self.snapshot.owner_graph;
        KubePodInfoV1::from_pod(self.api, owner_graph, self.settings)
    }
}

impl AggregationHost for Pod<'_> {
    fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    fn pods(&self) -> impl Iterator<Item = &Arc<v1::Pod>> {
        std::iter::once(self.api)
    }
}

impl PiggybackHost for Pod<'_> {
    fn metadata(&self) -> Option<&ObjectMeta> {
        Some(&self.api.metadata)
    }

    fn kind(&self) -> &str {
        &self.meta.kind
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();
        out.extend(self.performance_and_resource_util_sections(&me));

        if let Some(kube_pod_info_v1) = &self.info() {
            out.push(WriteableSection::of(&me, kube_pod_info_v1));
        }

        if self.settings.emit_pvc_sections
            && let Some(namespace) = self.meta.namespace
        {
            out.extend(self.pvc_sections(&me, namespace));
        }
        if let Some(phase) = &self.api.status.as_ref().and_then(|s| s.phase.as_deref()) {
            out.push(WriteableSection::of(&me, &KubePodLifecycleV1::new(phase)));
        }
        if let Some(kube_start_time_v1) = KubeStartTimeV1::from_pod(self.api) {
            out.push(WriteableSection::of(&me, &kube_start_time_v1));
        }
        if let Some(kube_pod_conditions_v1) = KubePodConditionsV1::from_pod(self.api) {
            out.push(WriteableSection::of(&me, &kube_pod_conditions_v1));
        }
        if let Some(kube_pod_containers_v1) = KubePodContainersV1::from_pod(self.api) {
            out.push(WriteableSection::of(&me, &kube_pod_containers_v1));
        }
        if let Some(kube_pod_init_containers_v1) = KubePodInitContainersV1::from_pod(self.api) {
            out.push(WriteableSection::of(&me, &kube_pod_init_containers_v1));
        }
        if let Some(kube_pod_container_specs_v1) = KubePodContainerSpecsV1::from_pod(self.api) {
            out.push(WriteableSection::of(&me, &kube_pod_container_specs_v1));
        }
        if let Some(kube_pod_init_container_specs_v1) =
            KubePodInitContainerSpecsV1::from_pod(self.api)
        {
            out.push(WriteableSection::of(&me, &kube_pod_init_container_specs_v1));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::section::writeable::SectionBody;
    use crate::state::tests::test_app_state;
    use crate::test_support::{host_settings, pod_prefilled};

    #[test]
    fn pod_omits_pod_resources_rollup() {
        let state = test_app_state();
        let snapshot = state.snapshot();
        let api = Arc::new(pod_prefilled("pod-1"));
        let settings = host_settings();
        let Some(pod) = Pod::new(&api, &snapshot, &settings) else {
            panic!("valid test Pod should become a piggyback host");
        };

        let sections = pod.emit();
        assert!(sections.iter().all(Result::is_ok));
        let has_section = |expected| {
            sections.iter().any(|section| {
                matches!(section, Ok(WriteableSection {
                    body: SectionBody::Json { name, .. },
                    ..
                }) if *name == expected)
            })
        };
        assert!(has_section("kube_cpu_resources_v1"));
        assert!(has_section("kube_memory_resources_v1"));
        assert!(!has_section("kube_pod_resources_v1"));
    }
}
