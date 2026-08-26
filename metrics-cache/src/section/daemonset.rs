use k8s_openapi::api::apps::v1::DaemonSet;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::{LabelRef, Selector, ThinContainers};

#[derive(Serialize)]
pub(crate) struct KubeDaemonSetInfoV1<'a> {
    pub name: &'a str,
    pub namespace: &'a str,
    pub creation_timestamp: Option<f64>,
    pub labels: BTreeMap<&'a str, LabelRef<'a>>,
    pub annotations: BTreeMap<&'a str, &'a str>,
    pub selector: Selector<'a>,
    pub containers: ThinContainers<'a>,
    pub cluster: &'a str,
    pub kubernetes_cluster_hostname: &'a str,
}

impl<'a> KubeDaemonSetInfoV1<'a> {
    pub(crate) fn from_daemonset(
        daemonset: &'a DaemonSet,
        containers: ThinContainers<'a>,
        settings: &'a HostSettings,
    ) -> Option<Self> {
        let spec = daemonset.spec.as_ref()?;
        Some(Self {
            name: daemonset.metadata.name.as_deref()?,
            namespace: daemonset.metadata.namespace.as_deref()?,
            creation_timestamp: daemonset
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: daemonset
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: daemonset
                .metadata
                .annotations
                .as_ref()
                .map(|annotations| settings.annotation_key_pattern.filter(annotations))
                .unwrap_or_default(),
            selector: Selector::from_label_selector(&spec.selector),
            containers,
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        })
    }
}

impl Section for KubeDaemonSetInfoV1<'_> {
    const NAME: &'static str = "kube_daemonset_info_v1";
}

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
    use std::sync::Arc;

    use crate::test_support::{daemonset, host_settings, pod_with_container_statuses};

    #[test]
    fn kube_daemonset_info_v1() {
        let daemonset = daemonset("node-agent");
        let pods = [Arc::new(pod_with_container_statuses(
            "node-agent-1",
            &[("agent", "agent:1.0"), ("sidecar", "sidecar:2.0")],
        ))];

        insta::assert_json_snapshot!(KubeDaemonSetInfoV1::from_daemonset(
            &daemonset,
            ThinContainers::from_pods(pods.iter()),
            &host_settings(),
        ));
    }

    #[test]
    fn kube_daemonset_replicas_v1() {
        let mut daemonset = daemonset("node-agent");
        daemonset.status = Some(DaemonSetStatus {
            number_available: Some(8),
            desired_number_scheduled: 10,
            number_ready: 7,
            updated_number_scheduled: Some(9),
            number_misscheduled: 2,
            ..Default::default()
        });

        insta::assert_json_snapshot!(KubeDaemonSetReplicasV1::from_daemonset(&daemonset));
    }
}
