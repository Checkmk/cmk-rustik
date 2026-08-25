use k8s_openapi::api::batch::v1::{CronJob, Job, JobCondition as ApiJobCondition};
use k8s_openapi::api::core::v1::Pod;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::LabelRef;
use crate::section::container::ContainerStatusValue;

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

/// The most recently created Job belonging to a CronJob.
/// (`kube_cron_job_latest_job_v1`)
#[derive(Serialize)]
pub(crate) struct KubeCronJobLatestJobV1<'a> {
    status: JobStatus,
    pods: Vec<JobPod<'a>>,
}

#[derive(Serialize)]
struct JobStatus {
    conditions: Vec<JobCondition>,
    start_time: Option<f64>,
    completion_time: Option<f64>,
}

#[derive(Serialize)]
struct JobCondition {
    type_: String,
    status: String,
}

#[derive(Serialize)]
struct JobPod<'a> {
    init_containers: BTreeMap<&'a str, ContainerStatusValue<'a>>,
    containers: BTreeMap<&'a str, ContainerStatusValue<'a>>,
    lifecycle: PodLifecycle,
}

#[derive(Serialize)]
struct PodLifecycle {
    phase: String,
}

impl<'a> KubeCronJobLatestJobV1<'a> {
    pub(crate) fn from_job(job: &'a Job, pods: impl Iterator<Item = &'a Pod>) -> Option<Self> {
        let status = job.status.as_ref()?;
        let conditions =
            JobCondition::from_conditions(status.conditions.as_deref().unwrap_or_default());
        let pods = pods.filter_map(JobPod::from_pod).collect();

        Some(Self {
            status: JobStatus {
                conditions,
                start_time: status
                    .start_time
                    .as_ref()
                    .map(|time| time.0.as_millisecond() as f64 / 1000.0),
                completion_time: status
                    .completion_time
                    .as_ref()
                    .map(|time| time.0.as_millisecond() as f64 / 1000.0),
            },
            pods,
        })
    }
}

impl JobCondition {
    fn from_conditions(conditions: &[ApiJobCondition]) -> Vec<Self> {
        let mut seen = HashSet::new();
        conditions
            .iter()
            .filter(|condition| {
                seen.insert((
                    condition.type_.as_str(),
                    condition
                        .last_probe_time
                        .as_ref()
                        .map(|time| time.0.to_string()),
                ))
            })
            .map(|condition| Self {
                type_: capitalize(&condition.type_),
                status: capitalize(&condition.status),
            })
            .collect()
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

impl<'a> JobPod<'a> {
    fn from_pod(pod: &'a Pod) -> Option<Self> {
        let status = pod.status.as_ref()?;
        Some(Self {
            init_containers: status
                .init_container_statuses
                .as_deref()
                .map(ContainerStatusValue::from_statuses)
                .unwrap_or_default(),
            containers: status
                .container_statuses
                .as_deref()
                .map(ContainerStatusValue::from_statuses)
                .unwrap_or_default(),
            lifecycle: PodLifecycle {
                phase: status.phase.as_deref()?.to_lowercase(),
            },
        })
    }
}

impl Section for KubeCronJobLatestJobV1<'_> {
    const NAME: &'static str = "kube_cron_job_latest_job_v1";
}

/// The newest Job owned by this CronJob whose
/// `status.completion_time` is set.
fn last_completed_job(jobs: &[Arc<Job>]) -> Option<&Arc<Job>> {
    jobs.iter()
        .filter(|job| {
            job.status
                .as_ref()
                .and_then(|s| s.completion_time.as_ref())
                .is_some()
        })
        .max_by_key(|job| {
            job.metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond())
        })
}

fn job_duration(job: &Job) -> Option<f64> {
    let status = job.status.as_ref()?;
    let completion_time = status.completion_time.as_ref()?;
    let start_time = status.start_time.as_ref()?;
    Some((completion_time.0.as_millisecond() - start_time.0.as_millisecond()) as f64 / 1000.0)
}

/// CronJob status. (`kube_cron_job_status_v1`)
#[derive(Serialize)]
pub(crate) struct KubeCronJobStatusV1 {
    pub active_jobs_count: Option<i32>,
    pub last_duration: Option<f64>,
    pub last_successful_time: Option<f64>,
    pub last_schedule_time: Option<f64>,
}

impl KubeCronJobStatusV1 {
    pub fn from_cron_job(cron_job: &CronJob, jobs: &[Arc<Job>]) -> Self {
        let status = cron_job.status.as_ref();
        Self {
            active_jobs_count: status
                .and_then(|s| s.active.as_ref())
                .filter(|active| !active.is_empty())
                .map(|active| active.len() as i32),
            last_duration: last_completed_job(jobs).and_then(|job| job_duration(job)),
            last_successful_time: status
                .and_then(|s| s.last_successful_time.as_ref())
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            last_schedule_time: status
                .and_then(|s| s.last_schedule_time.as_ref())
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
        }
    }
}

impl Section for KubeCronJobStatusV1 {
    const NAME: &'static str = "kube_cron_job_status_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::batch::v1::{
        CronJobStatus, JobCondition as ApiJobCondition, JobStatus as ApiJobStatus,
    };
    use k8s_openapi::api::core::v1::{
        ContainerState, ContainerStateRunning, ContainerStateWaiting, ContainerStatus,
        ObjectReference, PodStatus,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
    use k8s_openapi::jiff::Timestamp;
    use regex::Regex;

    use crate::host_settings::AnnotationKeyPattern;
    use crate::test_support::{cron_job, host_settings, job_owned_by, owner_ref};

    fn timestamp(s: &str) -> Time {
        let timestamp: Timestamp = s.parse().unwrap();
        Time(timestamp)
    }

    fn completed_job(name: &str, created_at: &str, started_at: &str, completed_at: &str) -> Job {
        let mut job = job_owned_by(name, name, owner_ref("CronJob", "important-job", "cj-uid"));
        job.metadata.creation_timestamp = Some(timestamp(created_at));
        job.status = Some(ApiJobStatus {
            start_time: Some(timestamp(started_at)),
            completion_time: Some(timestamp(completed_at)),
            ..Default::default()
        });
        job
    }

    fn container_status(name: &str, state: ContainerState) -> ContainerStatus {
        ContainerStatus {
            container_id: Some(format!("containerd://{name}")),
            image_id: format!("{name}-image-id"),
            name: name.into(),
            image: format!("{name}:latest"),
            ready: true,
            state: Some(state),
            ..Default::default()
        }
    }

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

    #[test]
    fn kube_cron_job_latest_job_v1() {
        let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
        let job = Job {
            status: Some(ApiJobStatus {
                conditions: Some(vec![ApiJobCondition {
                    type_: "Complete".into(),
                    status: "True".into(),
                    ..Default::default()
                }]),
                start_time: Some(Time(timestamp.clone())),
                completion_time: Some(Time(timestamp)),
                ..Default::default()
            }),
            ..Default::default()
        };
        let pod = Pod {
            status: Some(PodStatus {
                phase: Some("Succeeded".into()),
                init_container_statuses: Some(vec![container_status(
                    "init",
                    ContainerState {
                        running: Some(ContainerStateRunning {
                            started_at: Some(Time("2024-06-19 15:22:46-04".parse().unwrap())),
                        }),
                        ..Default::default()
                    },
                )]),
                container_statuses: Some(vec![container_status(
                    "main",
                    ContainerState {
                        waiting: Some(ContainerStateWaiting {
                            reason: Some("PodInitializing".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            }),
            ..Default::default()
        };

        insta::assert_json_snapshot!(KubeCronJobLatestJobV1::from_job(&job, [&pod].into_iter()));
    }

    #[test]
    fn kube_cron_job_status_v1() {
        let mut cron_job = cron_job("important-job");
        cron_job.status = Some(CronJobStatus {
            active: Some(vec![ObjectReference::default(), ObjectReference::default()]),
            last_schedule_time: Some(timestamp("2024-06-19 16:00:00-04")),
            last_successful_time: Some(timestamp("2024-06-19 15:30:00-04")),
        });
        let jobs: [Arc<Job>; 2] = [
            completed_job(
                "job-1",
                "2024-06-19 15:00:00-04",
                "2024-06-19 15:00:05-04",
                "2024-06-19 15:00:35-04",
            )
            .into(),
            completed_job(
                "job-2",
                "2024-06-19 15:30:00-04",
                "2024-06-19 15:30:05-04",
                "2024-06-19 15:30:45-04",
            )
            .into(),
        ];
        insta::assert_json_snapshot!(KubeCronJobStatusV1::from_cron_job(&cron_job, &jobs));
    }

    /// Regression test for the fix of CMK-36468.
    #[test]
    fn kube_cron_job_status_v1_picks_newest_completed_job_not_oldest() {
        let cron_job = cron_job("important-job");
        let older = completed_job(
            "older-job",
            "2024-06-19 15:00:00-04",
            "2024-06-19 15:00:00-04",
            "2024-06-19 15:00:10-04",
        ); // 10s duration
        let newer = completed_job(
            "newer-job",
            "2024-06-19 15:30:00-04",
            "2024-06-19 15:30:00-04",
            "2024-06-19 15:31:40-04",
        ); // 100s duration
        let jobs: [Arc<Job>; 2] = [newer.into(), older.into()];
        let status = KubeCronJobStatusV1::from_cron_job(&cron_job, &jobs);
        assert_eq!(status.last_duration, Some(100.0));
    }

    #[test]
    fn kube_cron_job_status_v1_ignores_incomplete_jobs() {
        let cron_job = cron_job("important-job");
        let completed = completed_job(
            "completed-job",
            "2024-06-19 15:00:00-04",
            "2024-06-19 15:00:00-04",
            "2024-06-19 15:00:10-04",
        ); // 10s duration
        let mut incomplete = job_owned_by(
            "incomplete-job",
            "incomplete-job",
            owner_ref("CronJob", "important-job", "cj-uid"),
        );
        incomplete.metadata.creation_timestamp = Some(timestamp("2024-06-19 15:30:00-04"));
        incomplete.status = Some(ApiJobStatus {
            start_time: Some(timestamp("2024-06-19 15:30:00-04")),
            completion_time: None,
            ..Default::default()
        });
        let jobs: [Arc<Job>; 2] = [incomplete.into(), completed.into()];
        let status = KubeCronJobStatusV1::from_cron_job(&cron_job, &jobs);
        assert_eq!(status.last_duration, Some(10.0));
    }

    #[test]
    fn kube_cron_job_status_v1_active_jobs_count_zero_is_none() {
        let mut cron_job = cron_job("important-job");
        cron_job.status = Some(CronJobStatus {
            active: Some(vec![]),
            ..Default::default()
        });
        let status = KubeCronJobStatusV1::from_cron_job(&cron_job, &[]);
        assert_eq!(status.active_jobs_count, None);
    }

    #[test]
    fn kube_cron_job_status_v1_without_status_or_jobs() {
        let cron_job = cron_job("important-job");
        let status = KubeCronJobStatusV1::from_cron_job(&cron_job, &[]);
        assert_eq!(status.active_jobs_count, None);
        assert_eq!(status.last_duration, None);
        assert_eq!(status.last_successful_time, None);
        assert_eq!(status.last_schedule_time, None);
    }
}
