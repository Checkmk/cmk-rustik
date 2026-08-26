use k8s_openapi::api::apps::v1::{Deployment, DeploymentCondition};
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

#[derive(Serialize)]
struct DeploymentConditionValue<'a> {
    status: &'a str,
    last_transition_time: f64,
    reason: &'a str,
    message: &'a str,
}

impl<'a> DeploymentConditionValue<'a> {
    fn from_condition(condition: &'a DeploymentCondition) -> Option<Self> {
        let last_transition_time = condition.last_transition_time.as_ref()?;
        Some(Self {
            status: &condition.status,
            last_transition_time: last_transition_time.0.as_millisecond() as f64 / 1000.0,
            reason: condition.reason.as_deref()?,
            message: condition.message.as_deref()?,
        })
    }
}

/// Deployment conditions. (`kube_deployment_conditions_v1`)
#[derive(Default, Serialize)]
pub(crate) struct KubeDeploymentConditionsV1<'a> {
    available: Option<DeploymentConditionValue<'a>>,
    progressing: Option<DeploymentConditionValue<'a>>,
    replicafailure: Option<DeploymentConditionValue<'a>>,
}

impl<'a> KubeDeploymentConditionsV1<'a> {
    pub(crate) fn from_deployment(deployment: &'a Deployment) -> Option<Self> {
        let conditions = deployment.status.as_ref()?.conditions.as_deref()?;
        if conditions.is_empty() {
            return None;
        }

        let mut section = Self::default();
        for condition in conditions {
            let target = if condition.type_.eq_ignore_ascii_case("Available") {
                &mut section.available
            } else if condition.type_.eq_ignore_ascii_case("Progressing") {
                &mut section.progressing
            } else if condition.type_.eq_ignore_ascii_case("ReplicaFailure") {
                &mut section.replicafailure
            } else {
                continue;
            };
            *target = Some(DeploymentConditionValue::from_condition(condition)?);
        }
        Some(section)
    }
}

impl Section for KubeDeploymentConditionsV1<'_> {
    const NAME: &'static str = "kube_deployment_conditions_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::apps::v1::{DeploymentCondition, DeploymentStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
    use k8s_openapi::jiff::Timestamp;
    use std::sync::Arc;

    use crate::test_support::{deployment, host_settings, pod_with_container_statuses};

    fn condition(type_: &str, status: &str, reason: &str, message: &str) -> DeploymentCondition {
        DeploymentCondition {
            type_: type_.to_owned(),
            status: status.to_owned(),
            last_transition_time: Some(Time(
                "2024-06-19 15:22:45-04".parse::<Timestamp>().unwrap(),
            )),
            reason: Some(reason.to_owned()),
            message: Some(message.to_owned()),
            ..Default::default()
        }
    }

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

    #[test]
    fn kube_deployment_conditions_v1() {
        let mut deployment = deployment("nginx");
        deployment.status = Some(DeploymentStatus {
            conditions: Some(vec![
                condition("Available", "True", "MinimumReplicasAvailable", "ready"),
                condition("progressing", "True", "NewReplicaSetAvailable", "complete"),
                condition("ReplicaFailure", "False", "", ""),
            ]),
            ..Default::default()
        });

        insta::assert_json_snapshot!(KubeDeploymentConditionsV1::from_deployment(&deployment));
    }

    #[test]
    fn kube_deployment_conditions_v1_without_conditions() {
        let mut deployment = deployment("nginx");
        deployment.status = Some(DeploymentStatus::default());

        assert!(KubeDeploymentConditionsV1::from_deployment(&deployment).is_none());
    }
}
