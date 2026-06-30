use k8s_openapi::api::core::v1;

use crate::host_settings::HostSettings;
use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1},
    pod::{KubePodInfoV1, KubePodLifecycleV1},
    pvc::{KubePvcPvsV1, KubePvcV1, KubePvcVolumesV1},
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

impl<'a> Pod<'a> {
    pub fn new(
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
    fn info(&'a self) -> Option<KubePodInfoV1<'a>> {
        let owner_graph = &self.snapshot.owner_graph;
        KubePodInfoV1::from_pod(self.api, owner_graph, self.settings)
    }

    /// Generate the section `kube_pvc_v1`, which includes each volume attached
    /// to the pod which has a corresponding PVC in the snapshot.
    fn pvcs(&'a self) -> Option<KubePvcV1<'a>> {
        let claim_names = KubePvcV1::pod_pvc_claim_names(self.api);
        KubePvcV1::from_claim_names(self.snapshot, self.meta.namespace?, claim_names)
    }

    /// Generate the section `kube_pvc_volumes_v1` which extends PVC information
    /// with live usage metrics (capacity/free space).
    fn pvc_volumes(&'a self) -> Option<KubePvcVolumesV1<'a>> {
        let claim_names = KubePvcV1::pod_pvc_claim_names(self.api);
        KubePvcVolumesV1::from_claim_names(self.snapshot, self.meta.namespace?, claim_names)
    }

    /// Generate the section `kube_pvc_pvs_v1` which extends PVC information
    /// with PV information.
    fn pvs(&'a self) -> Option<KubePvcPvsV1<'a>> {
        let claim_names = KubePvcV1::pod_pvc_claim_names(self.api);
        KubePvcPvsV1::from_claim_names(self.snapshot, self.meta.namespace?, claim_names)
    }
}

impl PiggybackHost for Pod<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);

        // kube_pod_info_v1
        let mut out = Vec::new();

        if let Some(kube_pod_info_v1) = &self.info() {
            out.push(WriteableSection::of(me.clone(), kube_pod_info_v1));
        }

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
            out.push(WriteableSection::of(me.clone(), kube_pvc_v1));
        };
        if let Some(kube_pvc_volumes_v1) = &self.pvc_volumes() {
            out.push(WriteableSection::of(me.clone(), kube_pvc_volumes_v1));
        };
        if let Some(kube_pvc_pvs_v1) = &self.pvs() {
            out.push(WriteableSection::of(me.clone(), kube_pvc_pvs_v1));
        };
        if let Some(phase) = &self.api.status.as_ref().and_then(|s| s.phase.as_deref()) {
            out.push(WriteableSection::of(me, &KubePodLifecycleV1::new(phase)));
        }

        out
    }
}
