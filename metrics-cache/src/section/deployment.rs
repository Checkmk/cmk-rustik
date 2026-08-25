use k8s_openapi::api::apps::v1::Deployment;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::{LabelRef, Selector, ThinContainers};

#[derive(Serialize)]
pub(crate) struct KubeDeploymentInfoV1<'a> {
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

impl<'a> KubeDeploymentInfoV1<'a> {
    pub fn from_deployment(
        deployment: &'a Deployment,
        containers: ThinContainers<'a>,
        settings: &'a HostSettings,
    ) -> Option<KubeDeploymentInfoV1<'a>> {
        let spec = deployment.spec.as_ref()?;
        let section = KubeDeploymentInfoV1 {
            name: deployment.metadata.name.as_deref()?,
            namespace: deployment.metadata.namespace.as_deref()?,
            creation_timestamp: deployment
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: deployment
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: deployment
                .metadata
                .annotations
                .as_ref()
                .map(|m| settings.annotation_key_pattern.filter(m))
                .unwrap_or_default(),
            selector: Selector::from_label_selector(&spec.selector),
            containers,
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        };
        Some(section)
    }
}

impl Section for KubeDeploymentInfoV1<'_> {
    const NAME: &'static str = "kube_deployment_info_v1";
}

/// Deployment replica counts. (`kube_deployment_replicas_v1`)
#[derive(Serialize)]
pub(crate) struct KubeDeploymentReplicasV1 {
    available: i32,
    desired: i32,
    ready: i32,
    updated: i32,
    terminating: Option<i32>,
}

impl KubeDeploymentReplicasV1 {
    pub(crate) fn from_deployment(deployment: &Deployment) -> Option<Self> {
        let spec = deployment.spec.as_ref()?;
        let status = deployment.status.as_ref()?;
        Some(Self {
            available: status.available_replicas.unwrap_or(0),
            desired: spec.replicas?,
            ready: status.ready_replicas.unwrap_or(0),
            updated: status.updated_replicas.unwrap_or(0),
            terminating: status.terminating_replicas,
        })
    }
}

impl Section for KubeDeploymentReplicasV1 {
    const NAME: &'static str = "kube_deployment_replicas_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::apps::v1::DeploymentStatus;
    use std::sync::Arc;

    use crate::test_support::{deployment, host_settings, pod_with_container_statuses};

    #[test]
    fn kube_deployment_info_v1() {
        let deployment = deployment("nginx");
        let pods = [
            Arc::new(pod_with_container_statuses(
                "nginx-1",
                &[("nginx", "nginx:1.27")],
            )),
            Arc::new(pod_with_container_statuses(
                "nginx-2",
                &[("nginx", "nginx:1.27"), ("sidecar", "envoy:1.31")],
            )),
        ];
        let containers = ThinContainers::from_pods(pods.iter());
        insta::assert_json_snapshot!(KubeDeploymentInfoV1::from_deployment(
            &deployment,
            containers,
            &host_settings()
        ));
    }

    #[test]
    fn kube_deployment_info_v1_without_spec() {
        let mut deployment = deployment("nginx");
        deployment.spec = None;
        let containers = ThinContainers::from_pods(std::iter::empty());
        assert!(
            KubeDeploymentInfoV1::from_deployment(&deployment, containers, &host_settings())
                .is_none()
        );
    }

    #[test]
    fn kube_deployment_replicas_v1() {
        let mut deployment = deployment("nginx");
        deployment.spec.as_mut().unwrap().replicas = Some(5);
        deployment.status = Some(DeploymentStatus {
            available_replicas: Some(4),
            ready_replicas: Some(3),
            replicas: Some(99),
            updated_replicas: Some(2),
            terminating_replicas: Some(1),
            ..Default::default()
        });

        insta::assert_json_snapshot!(KubeDeploymentReplicasV1::from_deployment(&deployment));
    }

    #[test]
    fn deployment_replicas_default_missing_status_counts() {
        let mut deployment = deployment("nginx");
        deployment.spec.as_mut().unwrap().replicas = Some(0);
        deployment.status = Some(DeploymentStatus::default());

        let replicas = KubeDeploymentReplicasV1::from_deployment(&deployment).unwrap();
        assert_eq!(
            (replicas.available, replicas.ready, replicas.updated),
            (0, 0, 0)
        );
        assert_eq!(replicas.terminating, None);
    }
}
