use k8s_openapi::api::apps::v1::DaemonSet;
use serde::Serialize;

use crate::section::Section;

/// DaemonSet replica counts. (`kube_daemonset_replicas_v1`)
#[derive(Serialize)]
pub(crate) struct KubeDaemonSetReplicasV1 {
    available: i32,
    desired: i32,
    ready: i32,
    updated: i32,
    misscheduled: i32,
}

impl KubeDaemonSetReplicasV1 {
    pub(crate) fn from_daemonset(daemonset: &DaemonSet) -> Option<Self> {
        let status = daemonset.status.as_ref()?;
        Some(Self {
            available: status.number_available.unwrap_or(0),
            desired: status.desired_number_scheduled,
            ready: status.number_ready,
            updated: status.updated_number_scheduled.unwrap_or(0),
            misscheduled: status.number_misscheduled,
        })
    }
}

impl Section for KubeDaemonSetReplicasV1 {
    const NAME: &'static str = "kube_daemonset_replicas_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::apps::v1::DaemonSetStatus;

    #[test]
    fn kube_daemonset_replicas_v1() {
        let daemonset = DaemonSet {
            status: Some(DaemonSetStatus {
                number_available: Some(8),
                desired_number_scheduled: 10,
                number_ready: 7,
                updated_number_scheduled: Some(9),
                number_misscheduled: 2,
                ..Default::default()
            }),
            ..Default::default()
        };

        insta::assert_json_snapshot!(KubeDaemonSetReplicasV1::from_daemonset(&daemonset));
    }
}
