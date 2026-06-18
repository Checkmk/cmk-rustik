use k8s_openapi::api::core::v1;

use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    common::Controller,
    performance::{KubePerformanceCpuV1, KubePerformanceMemoryV1},
    pod::{KubePodInfoV1, QosClass},
    resource::{KubeCpuResourcesV1, KubeMemoryResourcesV1, ResourceAccumulator, ResourceAxis},
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct Pod<'a> {
    api: &'a v1::Pod,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
}

impl Pod<'_> {
    pub fn new<'a>(api: &'a v1::Pod, snapshot: &'a Snapshot) -> Option<Pod<'a>> {
        Some(Pod {
            api,
            meta: Meta::from_resource(api)?,
            snapshot,
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
            labels: std::collections::BTreeMap::new(), // TODO
            annotations: std::collections::BTreeMap::new(), // TODO
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
            cluster: "TODO_CLUSTERNAME",                     // TODO
            kubernetes_cluster_hostname: "TODO_CLUSTERNAME", // TODO
        }
    }
}

impl PiggybackHost for Pod<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname("TODO_CLUSTERNAME");

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
            me,
            &KubeMemoryResourcesV1(ResourceAccumulator::from_pod(
                self.api,
                ResourceAxis::Memory,
            )),
        ));

        out
    }
}
