use k8s_openapi::api::batch::v1;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::{
    cronjob::KubeCronJobInfoV1,
    writeable::{SectionError, WriteableSection},
};
use crate::snapshot::Snapshot;

pub struct CronJob<'a> {
    api: &'a v1::CronJob,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
    uid: &'a str,
}

impl CronJob<'_> {
    pub fn new<'a>(
        api: &'a v1::CronJob,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<CronJob<'a>> {
        let meta = Meta::from_resource(api)?;
        let uid = api.metadata.uid.as_deref()?;
        Some(CronJob {
            api,
            meta,
            snapshot,
            settings,
            uid,
        })
    }
}

impl AggregationHost for CronJob<'_> {
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

impl PiggybackHost for CronJob<'_> {
    fn metadata(&self) -> Option<&ObjectMeta> {
        Some(&self.api.metadata)
    }

    fn kind(&self) -> &str {
        &self.meta.kind
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();
        if let Some(kube_cron_job_info_v1) =
            KubeCronJobInfoV1::from_cron_job(self.api, self.settings)
        {
            out.push(WriteableSection::of(&me, &kube_cron_job_info_v1));
        }
        out.extend(self.aggregation_sections(&me));
        out
    }
}
