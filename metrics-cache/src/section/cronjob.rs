use k8s_openapi::api::batch::v1::CronJob;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::LabelRef;

/// CronJob info. (`kube_cron_job_info_v1`)
#[derive(Serialize)]
pub(crate) struct KubeCronJobInfoV1<'a> {
    pub name: &'a str,
    pub namespace: &'a str,
    pub creation_timestamp: Option<f64>,
    pub labels: BTreeMap<&'a str, LabelRef<'a>>,
    /// Annotations filtered with user input.
    ///
    /// After receiving the annotations from the Kubernetes API, we cannot
    /// process all of them as HostLabels. FilteredAnnotations are those
    /// annotations, which can be processed. This means that the annotations can
    /// no longer be arbitrary json objects and that options from the
    /// `Kubernetes` rule have been taken into account.
    pub annotations: BTreeMap<&'a str, &'a str>,
    pub schedule: &'a str,
    pub concurrency_policy: &'a str,
    pub failed_jobs_history_limit: i32,
    pub successful_jobs_history_limit: i32,
    pub suspend: bool,
    pub cluster: &'a str,
    pub kubernetes_cluster_hostname: &'a str,
}

impl<'a> KubeCronJobInfoV1<'a> {
    pub fn from_cron_job(
        cron_job: &'a CronJob,
        settings: &'a HostSettings,
    ) -> Option<KubeCronJobInfoV1<'a>> {
        let section = KubeCronJobInfoV1 {
            name: cron_job.metadata.name.as_deref()?,
            namespace: cron_job.metadata.namespace.as_deref()?,
            creation_timestamp: cron_job
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: cron_job
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: cron_job
                .metadata
                .annotations
                .as_ref()
                .map(|m| settings.annotation_key_pattern.filter(m))
                .unwrap_or_default(),
            schedule: &cron_job.spec.schedule,
            concurrency_policy: cron_job.spec.concurrency_policy.as_deref()?,
            failed_jobs_history_limit: cron_job.spec.failed_jobs_history_limit?,
            successful_jobs_history_limit: cron_job.spec.successful_jobs_history_limit?,
            suspend: cron_job.spec.suspend?,
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        };
        Some(section)
    }
}

impl Section for KubeCronJobInfoV1<'_> {
    const NAME: &'static str = "kube_cron_job_info_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    use crate::host_settings::AnnotationKeyPattern;
    use crate::test_support::{cron_job, host_settings};

    #[test]
    fn kube_cron_job_info_v1() {
        let cron_job = cron_job("important-job");
        let mut settings = host_settings();
        insta::assert_json_snapshot!(KubeCronJobInfoV1::from_cron_job(&cron_job, &settings));

        // This pattern should only match one annotation
        settings.annotation_key_pattern =
            AnnotationKeyPattern::Pattern(Regex::new("^example").unwrap());
        assert_eq!(
            KubeCronJobInfoV1::from_cron_job(&cron_job, &settings)
                .unwrap()
                .annotations
                .len(),
            1
        );

        // Ignore all annotations, emit 0 of them
        settings.annotation_key_pattern = AnnotationKeyPattern::IgnoreAll;
        assert_eq!(
            KubeCronJobInfoV1::from_cron_job(&cron_job, &settings)
                .unwrap()
                .annotations
                .len(),
            0
        );

        // Import all annotations (fixture default, captured by insta above, but let's be explicit)
        settings.annotation_key_pattern = AnnotationKeyPattern::ImportAll;
        assert_eq!(
            KubeCronJobInfoV1::from_cron_job(&cron_job, &settings)
                .unwrap()
                .annotations
                .len(),
            2
        );
    }
}
