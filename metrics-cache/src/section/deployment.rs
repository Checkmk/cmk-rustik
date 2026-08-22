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
