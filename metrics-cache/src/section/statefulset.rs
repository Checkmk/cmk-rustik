use k8s_openapi::api::apps::v1::StatefulSet;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::{LabelRef, Selector, ThinContainers};

#[derive(Serialize)]
pub(crate) struct KubeStatefulSetInfoV1<'a> {
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

impl<'a> KubeStatefulSetInfoV1<'a> {
    pub(crate) fn from_statefulset(
        statefulset: &'a StatefulSet,
        containers: ThinContainers<'a>,
        settings: &'a HostSettings,
    ) -> Option<Self> {
        let spec = statefulset.spec.as_ref()?;
        Some(Self {
            name: statefulset.metadata.name.as_deref()?,
            namespace: statefulset.metadata.namespace.as_deref()?,
            creation_timestamp: statefulset
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: statefulset
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: statefulset
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

impl Section for KubeStatefulSetInfoV1<'_> {
    const NAME: &'static str = "kube_statefulset_info_v1";
}

/// StatefulSet replica counts. (`kube_statefulset_replicas_v1`)
#[derive(Serialize)]
pub(crate) struct KubeStatefulSetReplicasV1 {
    available: i32,
    desired: i32,
    ready: i32,
    updated: i32,
}

impl KubeStatefulSetReplicasV1 {
    pub(crate) fn from_statefulset(statefulset: &StatefulSet) -> Option<Self> {
        let spec = statefulset.spec.as_ref()?;
        let status = statefulset.status.as_ref()?;
        Some(Self {
            available: status.available_replicas.unwrap_or(0),
            desired: spec.replicas?,
            ready: status.ready_replicas.unwrap_or(0),
            updated: status.updated_replicas.unwrap_or(0),
        })
    }
}

impl Section for KubeStatefulSetReplicasV1 {
    const NAME: &'static str = "kube_statefulset_replicas_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::apps::v1::StatefulSetStatus;
    use std::sync::Arc;

    use crate::test_support::{host_settings, pod_with_container_statuses, statefulset};

    #[test]
    fn kube_statefulset_info_v1() {
        let statefulset = statefulset("database");
        let pods = [Arc::new(pod_with_container_statuses(
            "database-0",
            &[
                ("postgres", "postgres:16"),
                ("exporter", "postgres-exporter:0.15"),
            ],
        ))];

        insta::assert_json_snapshot!(KubeStatefulSetInfoV1::from_statefulset(
            &statefulset,
            ThinContainers::from_pods(pods.iter()),
            &host_settings(),
        ));
    }

    #[test]
    fn kube_statefulset_info_v1_without_spec() {
        let mut statefulset = statefulset("database");
        statefulset.spec = None;
        assert!(
            KubeStatefulSetInfoV1::from_statefulset(
                &statefulset,
                ThinContainers::from_pods(std::iter::empty()),
                &host_settings(),
            )
            .is_none()
        )
    }

    #[test]
    fn kube_statefulset_replicas_v1() {
        let mut statefulset = statefulset("database");
        statefulset.spec.as_mut().unwrap().replicas = Some(5);
        statefulset.status = Some(StatefulSetStatus {
            available_replicas: Some(4),
            ready_replicas: Some(3),
            replicas: 99,
            updated_replicas: Some(2),
            ..Default::default()
        });

        insta::assert_json_snapshot!(KubeStatefulSetReplicasV1::from_statefulset(&statefulset));
    }

    #[test]
    fn statefulset_replicas_default_missing_status_counts() {
        let mut statefulset = statefulset("database");
        statefulset.spec.as_mut().unwrap().replicas = Some(0);
        statefulset.status = Some(StatefulSetStatus::default());

        let replicas = KubeStatefulSetReplicasV1::from_statefulset(&statefulset).unwrap();
        assert_eq!(
            (replicas.available, replicas.ready, replicas.updated),
            (0, 0, 0)
        );
    }
}
