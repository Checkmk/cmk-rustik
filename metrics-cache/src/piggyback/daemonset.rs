use k8s_openapi::api::apps::v1;
use k8s_openapi::api::core::v1::Pod;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct DaemonSet<'a> {
    _api: &'a v1::DaemonSet,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
    uid: &'a str,
}

impl DaemonSet<'_> {
    pub fn new<'a>(
        api: &'a v1::DaemonSet,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<DaemonSet<'a>> {
        let meta = Meta::from_resource(api)?;
        let uid = api.metadata.uid.as_deref()?;
        Some(DaemonSet {
            _api: api,
            meta,
            snapshot,
            settings,
            uid,
        })
    }
}

impl AggregationHost for DaemonSet<'_> {
    fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    fn pods(&self) -> impl Iterator<Item = &Arc<Pod>> {
        self.snapshot
            .owner_graph
            .pods_by_controller(self.uid)
            .iter()
    }
}

impl PiggybackHost for DaemonSet<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();
        out.extend(self.aggregation_sections(&me));
        out
    }
}
