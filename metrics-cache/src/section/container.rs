use k8s_openapi::api::core::v1::ContainerStatus;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use serde::Serialize;
use std::collections::BTreeMap;

fn to_unix_seconds(time: &Time) -> i64 {
    time.0.as_millisecond() / 1000
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub(crate) enum ContainerStateValue<'a> {
    #[serde(rename = "terminated")]
    Terminated {
        exit_code: i32,
        start_time: Option<i64>,
        end_time: Option<i64>,
        reason: Option<&'a str>,
        detail: Option<&'a str>,
    },
    #[serde(rename = "running")]
    Running { start_time: i64 },
    #[serde(rename = "waiting")]
    Waiting {
        reason: Option<&'a str>,
        detail: Option<&'a str>,
    },
}

/// A single container's status.
#[derive(Serialize)]
pub(crate) struct ContainerStatusValue<'a> {
    pub container_id: Option<&'a str>,
    pub image_id: &'a str,
    pub name: &'a str,
    pub image: &'a str,
    pub ready: bool,
    pub state: ContainerStateValue<'a>,
    pub restart_count: i32,
}

impl<'a> ContainerStatusValue<'a> {
    fn from_status(status: &'a ContainerStatus) -> Option<Self> {
        let state = status.state.as_ref()?;
        let state = if let Some(terminated) = &state.terminated {
            ContainerStateValue::Terminated {
                exit_code: terminated.exit_code,
                start_time: terminated.started_at.as_ref().map(to_unix_seconds),
                end_time: terminated.finished_at.as_ref().map(to_unix_seconds),
                reason: terminated.reason.as_deref(),
                detail: terminated.message.as_deref(),
            }
        } else if let Some(running) = &state.running {
            ContainerStateValue::Running {
                start_time: to_unix_seconds(running.started_at.as_ref()?),
            }
        } else if let Some(waiting) = &state.waiting {
            ContainerStateValue::Waiting {
                reason: waiting.reason.as_deref(),
                detail: waiting.message.as_deref(),
            }
        } else {
            // K8s guarantees exactly one of terminated/running/waiting is set;
            // mirrors Python's AssertionError-worthy case by skipping this
            // container defensively rather than crashing.
            return None;
        };

        Some(Self {
            container_id: status.container_id.as_deref(),
            image_id: &status.image_id,
            name: &status.name,
            image: &status.image,
            ready: status.ready,
            state,
            restart_count: status.restart_count,
        })
    }

    /// Build a status map, keyed by container name, from a raw container
    /// status list.
    pub(crate) fn from_statuses(
        statuses: &'a [ContainerStatus],
    ) -> BTreeMap<&'a str, ContainerStatusValue<'a>> {
        let mut containers = BTreeMap::new();
        for status in statuses {
            if let Some(value) = Self::from_status(status) {
                containers.insert(status.name.as_str(), value);
            }
        }
        containers
    }
}
