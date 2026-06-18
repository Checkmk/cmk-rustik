#![allow(unused)]
use k8s_openapi::api::core::v1;

use crate::piggyback::{Meta, PiggybackHost};
use crate::section::{
    namespace::KubeNamespaceInfoV1,
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct Namespace<'a> {
    api: &'a v1::Namespace,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
}

impl Namespace<'_> {
    pub fn new<'a>(api: &'a v1::Namespace, snapshot: &'a Snapshot) -> Option<Namespace<'a>> {
        Some(Namespace {
            api,
            meta: Meta::from_resource(api)?,
            snapshot,
        })
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
            labels: std::collections::BTreeMap::new(), // TODO
            annotations: std::collections::BTreeMap::new(), // TODO
            cluster: "TODO_CLUSTERNAME",               // TODO
            kubernetes_cluster_hostname: "TODO_CLUSTERNAME", // TODO
        }
    }
}

impl PiggybackHost for Namespace<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname("TODO_CLUSTERNAME");
        vec![WriteableSection::of(me, &self.info())]
    }
}
