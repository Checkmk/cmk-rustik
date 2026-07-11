use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct Node<'a> {
    api: &'a v1::Node,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
}

impl Node<'_> {
    pub fn new<'a>(
        api: &'a v1::Node,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<Node<'a>> {
        let meta = Meta::from_resource(api)?;
        Some(Node {
            api,
            meta,
            snapshot,
            settings,
        })
    }
}

impl AggregationHost for Node<'_> {
    fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    fn pods(&self) -> impl Iterator<Item = &Arc<v1::Pod>> {
        self.snapshot.indexes.pods_by_node(self.meta.name).iter()
    }
}

impl PiggybackHost for Node<'_> {
    fn metadata(&self) -> Option<&ObjectMeta> {
        Some(&self.api.metadata)
    }

    fn kind(&self) -> &str {
        &self.meta.kind
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();
        out.extend(self.aggregation_sections(&me));
        out
    }
}
