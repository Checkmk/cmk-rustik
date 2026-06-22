use k8s_openapi::api::core::v1;

use crate::host_settings::HostSettings;
use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    common::{Controller, LabelRef},
    performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1},
    pod::{KubePodInfoV1, QosClass},
    pvc::KubePvcV1,
    resource::{KubeCpuResourcesV1, KubeMemoryResourcesV1, ResourceAccumulator, ResourceAxis},
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct Pod<'a> {
    api: &'a v1::Pod,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
}

impl Pod<'_> {
    pub fn new<'a>(
        api: &'a v1::Pod,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<Pod<'a>> {
        Some(Pod {
            api,
            meta: Meta::from_resource(api)?,
            snapshot,
            settings,
        })
    }

    /// Generate the section `kube_pod_info_v1` from a snapshot.
    fn info<'a>(&'a self) -> KubePodInfoV1<'a> {
        let control_chain = match &self.api.metadata.uid {
            Some(uid) => self
                .snapshot
                .owner_graph
                .walk_up(uid)
                .iter()
                .map(|o| Controller {
                    type_: &o.kind,
                    name: &o.name,
                })
                .collect(),
            None => Vec::new(),
        };

        KubePodInfoV1 {
            namespace: self.meta.namespace,
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
            node: self.api.spec.as_ref().and_then(|s| s.node_name.as_deref()),
            host_network: self.api.spec.as_ref().and_then(|s| s.host_network),
            dns_policy: self.api.spec.as_ref().and_then(|s| s.dns_policy.as_deref()),
            host_ip: self.api.status.as_ref().and_then(|s| s.host_ip.as_deref()),
            pod_ip: self.api.status.as_ref().and_then(|s| s.pod_ip.as_deref()),
            qos_class: self
                .api
                .status
                .as_ref()
                .and_then(|s| s.qos_class.as_deref())
                .and_then(QosClass::from_str),
            restart_policy: self
                .api
                .spec
                .as_ref()
                .and_then(|s| s.restart_policy.as_deref())
                .unwrap_or("Always"),
            uid: self.api.metadata.uid.as_deref().unwrap_or_default(),
            controllers: control_chain,
            cluster: &self.settings.cluster_name,
            kubernetes_cluster_hostname: &self.settings.cluster_host_name,
        }
    }

    /// Generate the section `kube_pvc_v1`, for each volume attached to the pod
    /// which has a corresponding PVC in the snapshot.
    fn pvcs<'a>(&'a self) -> Option<KubePvcV1<'a>> {
        let claim_names = KubePvcV1::pod_pvc_claim_names(self.api);
        KubePvcV1::from_claim_names(self.snapshot, self.meta.namespace?, claim_names)
    }
}

impl PiggybackHost for Pod<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);

        // kube_pod_info_v1
        let mut out = vec![WriteableSection::of(me.clone(), &self.info())];

        if let Some(namespace) = &self.meta.namespace
            && let Some(sample) = &self.snapshot.metrics.pod_usage(namespace, self.meta.name)
        {
            // kube_performance_cpu_v1
            out.push(WriteableSection::of(
                me.clone(),
                &KubePerformanceCpuV1::new(sample.cpu_usage_nano_cores),
            ));
            // kube_performance_memory_v1
            out.push(WriteableSection::of(
                me.clone(),
                &KubePerformanceMemoryV1::new(sample.memory_working_set_bytes),
            ));
        }

        out.push(WriteableSection::of(
            me.clone(),
            &KubeCpuResourcesV1(ResourceAccumulator::from_pod(self.api, ResourceAxis::Cpu)),
        ));
        out.push(WriteableSection::of(
            me.clone(),
            &KubeMemoryResourcesV1(ResourceAccumulator::from_pod(
                self.api,
                ResourceAxis::Memory,
            )),
        ));
        if let Some(kube_pvc_v1) = &self.pvcs() {
            out.push(WriteableSection::of(me, kube_pvc_v1));
        };

        out
    }
}
