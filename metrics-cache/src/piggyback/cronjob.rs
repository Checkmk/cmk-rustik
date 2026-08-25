use k8s_openapi::api::batch::v1::{self, Job};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::cronjob::{KubeCronJobInfoV1, KubeCronJobLatestJobV1};
use crate::section::writeable::{SectionError, WriteableSection};
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
        if let Some(kube_cron_job_latest_job_v1) = self.latest_job_section() {
            out.push(WriteableSection::of(&me, &kube_cron_job_latest_job_v1));
        }
        out
    }
}

impl CronJob<'_> {
    fn latest_job(&self) -> Option<&Arc<Job>> {
        Self::most_recent_job(self.snapshot.owner_graph.jobs_by_controller(self.uid))
    }

    fn most_recent_job(jobs: &[Arc<Job>]) -> Option<&Arc<Job>> {
        jobs.iter().max_by_key(|job| {
            job.metadata
                .creation_timestamp
                .as_ref()
                .map(|time| time.0.as_millisecond())
                .unwrap_or(0)
        })
    }

    fn latest_job_section(&self) -> Option<KubeCronJobLatestJobV1<'_>> {
        let job = self.latest_job()?;
        let job_uid = job.metadata.uid.as_deref()?;
        KubeCronJobLatestJobV1::from_job(
            job,
            self.snapshot
                .owner_graph
                .pods_by_controller(job_uid)
                .iter()
                .map(Arc::as_ref),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
    use k8s_openapi::jiff::Timestamp;

    #[test]
    fn selects_most_recent_job() {
        let old = Arc::new(Job::default());
        let mut recent = Job::default();
        recent.metadata.creation_timestamp =
            Some(Time("2024-06-19 15:22:45-04".parse::<Timestamp>().unwrap()));
        let recent = Arc::new(recent);

        assert!(Arc::ptr_eq(
            CronJob::most_recent_job(&[recent.clone(), old]).unwrap(),
            &recent
        ));
    }
}
