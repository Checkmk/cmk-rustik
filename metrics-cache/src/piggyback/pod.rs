use k8s_openapi::api::core::v1;

use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    common::Controller,
    performance::{
        KubePerformanceCpuV1, KubePerformanceMemoryV1, PerformanceFields, PerformanceType,
    },
    pod::KubePodInfoV1,
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct Pod<'a> {
    api: &'a v1::Pod,
    meta: Meta<'a>,
}

impl Pod<'_> {
    pub fn new<'a>(api: &'a v1::Pod, _snapshot: &Snapshot) -> Option<Pod<'a>> {
        Some(Pod {
            api,
            meta: Meta::from_resource(api)?,
        })
    }

    /// Generate the section `kube_pod_info_v1` from a snapshot.
    fn info<'a>(&'a self, snapshot: &'a Snapshot) -> KubePodInfoV1<'a> {
        let control_chain = match &self.api.metadata.uid {
            Some(uid) => snapshot
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
                .and_then(|s| s.qos_class.as_deref()),
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

    fn performance_cpu(&self, snapshot: &Snapshot) -> Option<KubePerformanceCpuV1> {
        let namespace = self.api.metadata.namespace.clone()?;
        let pod_name = self.api.metadata.name.clone()?;
        let sample = snapshot.metrics.pod_usage(&namespace, &pod_name)?;
        let section = KubePerformanceCpuV1 {
            performance: PerformanceFields {
                type_: PerformanceType::Cpu,
                usage: sample.cpu_usage_nano_cores as f64 / 1e9,
            },
        };
        Some(section)
    }

    fn performance_memory(&self, snapshot: &Snapshot) -> Option<KubePerformanceMemoryV1> {
        let namespace = self.api.metadata.namespace.clone()?;
        let pod_name = self.api.metadata.name.clone()?;
        let sample = snapshot.metrics.pod_usage(&namespace, &pod_name)?;
        let section = KubePerformanceMemoryV1 {
            performance: PerformanceFields {
                type_: PerformanceType::Memory,
                usage: sample.memory_working_set_bytes as f64,
            },
        };
        Some(section)
    }
}

impl PiggybackHost for Pod<'_> {
    fn emit(&self, snapshot: &Snapshot) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname("TODO_CLUSTERNAME");
        let mut out = vec![WriteableSection::of(me.clone(), &self.info(snapshot))];
        if let Some(kube_performance_cpu_v1) = self.performance_cpu(snapshot) {
            out.push(WriteableSection::of(me.clone(), &kube_performance_cpu_v1));
        }
        if let Some(kube_performance_memory_v1) = self.performance_memory(snapshot) {
            out.push(WriteableSection::of(
                me.clone(),
                &kube_performance_memory_v1,
            ));
        }

        out
    }
}
