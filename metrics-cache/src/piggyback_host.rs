use k8s_openapi::api::core::v1;

use crate::sections::{Controller, KubePodInfoV1};
use crate::snapshot::Snapshot;
use crate::writeable_section::{SectionError, WriteableSection};

/// Common, identifying data used for a given piggyback host type.
///
/// Mostly, this is used (via [`self::piggyback_hostname()`]) to generate the
/// piggyback hostname for a given resource.
struct Meta<'a> {
    name: &'a str,
    namespace: Option<&'a str>, // None for cluster-scoped kinds
    kind: String,
}

impl<'a> Meta<'a> {
    fn from_resource<K>(api: &'a K) -> Option<Self>
    where
        K: kube::Resource + k8s_openapi::Resource,
    {
        let meta = api.meta();
        Some(Meta {
            name: meta.name.as_deref()?,
            namespace: meta.namespace.as_deref(),
            kind: K::KIND.to_lowercase(),
        })
    }

    fn piggyback_hostname(&self, cluster: &str) -> String {
        match self.namespace {
            Some(namespace) => {
                format!("{}_{}_{}_{}", self.kind, cluster, namespace, self.name)
            }
            None => format!("{}_{}_{}", self.kind, cluster, self.name),
        }
    }
}

pub(crate) trait PiggybackHost {
    fn emit(&self, snapshot: &Snapshot) -> Vec<Result<WriteableSection, SectionError>>;
}

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
}

impl PiggybackHost for Pod<'_> {
    fn emit(&self, snapshot: &Snapshot) -> Vec<Result<WriteableSection, SectionError>> {
        vec![WriteableSection::of(
            self.meta.piggyback_hostname("TODO_CLUSTERNAME"),
            &self.info(&snapshot),
        )]
    }
}
