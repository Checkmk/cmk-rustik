use k8s_openapi::api::apps::v1;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::controller_spec::KubeControllerSpecV1;
use crate::section::daemonset::KubeDaemonSetReplicasV1;
use crate::section::update_strategy::KubeUpdateStrategyV1;
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct DaemonSet<'a> {
    api: &'a v1::DaemonSet,
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
            api,
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
    fn metadata(&self) -> Option<&ObjectMeta> {
        Some(&self.api.metadata)
    }

    fn kind(&self) -> &str {
        &self.meta.kind
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();
        if let Some(kube_update_strategy_v1) = KubeUpdateStrategyV1::from_daemonset(self.api) {
            out.push(WriteableSection::of(&me, &kube_update_strategy_v1));
        }
        if let Some(kube_daemonset_replicas_v1) = KubeDaemonSetReplicasV1::from_daemonset(self.api)
        {
            out.push(WriteableSection::of(&me, &kube_daemonset_replicas_v1));
        }
        let min_ready_seconds = self
            .api
            .spec
            .as_ref()
            .and_then(|spec| spec.min_ready_seconds);
        out.push(WriteableSection::of(
            &me,
            &KubeControllerSpecV1::new(min_ready_seconds),
        ));
        out.extend(self.aggregation_sections(&me));
        out
    }
}
