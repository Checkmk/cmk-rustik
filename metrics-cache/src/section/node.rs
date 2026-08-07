use k8s_openapi::api::core::v1::Node;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::LabelRef;

/// One entry of a Node's `status.addresses`.
///
/// Not `k8s_openapi`'s `NodeAddress`: that one serializes as `type`, while
/// Checkmk expects `type_`.
#[derive(Serialize)]
pub(crate) struct NodeAddressRef<'a> {
    address: &'a str,
    type_: &'a str,
}

/// Node info. (`kube_node_info_v1`)
#[derive(Serialize)]
pub(crate) struct KubeNodeInfoV1<'a> {
    pub architecture: &'a str,
    pub kernel_version: &'a str,
    pub os_image: &'a str,
    pub operating_system: &'a str,
    pub container_runtime_version: &'a str,
    pub name: &'a str,
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
    pub addresses: Vec<NodeAddressRef<'a>>,
    pub cluster: &'a str,
    pub kubernetes_cluster_hostname: &'a str,
}

impl<'a> KubeNodeInfoV1<'a> {
    pub fn from_node(node: &'a Node, settings: &'a HostSettings) -> Option<KubeNodeInfoV1<'a>> {
        let status = node.status.as_ref()?;
        let node_info = status.node_info.as_ref()?;
        let node_section = KubeNodeInfoV1 {
            architecture: &node_info.architecture,
            kernel_version: &node_info.kernel_version,
            os_image: &node_info.os_image,
            operating_system: &node_info.operating_system,
            container_runtime_version: &node_info.container_runtime_version,
            name: node.metadata.name.as_deref()?,
            creation_timestamp: node
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|x| x.0.as_millisecond() as f64 / 1000.0),
            labels: node
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: node
                .metadata
                .annotations
                .as_ref()
                .map(|x| settings.annotation_key_pattern.filter(x))
                .unwrap_or_default(),
            addresses: status
                .addresses
                .iter()
                .flatten()
                .map(|x| NodeAddressRef {
                    address: &x.address,
                    type_: &x.type_,
                })
                .collect(),
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        };
        Some(node_section)
    }
}
impl Section for KubeNodeInfoV1<'_> {
    const NAME: &'static str = "kube_node_info_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    use crate::host_settings::AnnotationKeyPattern;
    use crate::test_support::{host_settings, node, node_prefilled};

    #[test]
    fn kube_node_info_v1() {
        let node = node_prefilled("worker-1");
        let mut settings = host_settings();
        insta::assert_json_snapshot!(KubeNodeInfoV1::from_node(&node, &settings));

        // This pattern should only match one annotation
        settings.annotation_key_pattern =
            AnnotationKeyPattern::Pattern(Regex::new("^example").unwrap());
        assert_eq!(
            KubeNodeInfoV1::from_node(&node, &settings)
                .unwrap()
                .annotations
                .len(),
            1
        );

        // Ignore all annotations, emit 0 of them
        settings.annotation_key_pattern = AnnotationKeyPattern::IgnoreAll;
        assert_eq!(
            KubeNodeInfoV1::from_node(&node, &settings)
                .unwrap()
                .annotations
                .len(),
            0
        );

        // Import all annotations (fixture default, captured by insta above, but let's be explicit)
        settings.annotation_key_pattern = AnnotationKeyPattern::ImportAll;
        assert_eq!(
            KubeNodeInfoV1::from_node(&node, &settings)
                .unwrap()
                .annotations
                .len(),
            2
        );
    }

    /// A Node without `status.nodeInfo` cannot fill the five mandatory fields
    /// of the section, so we emit nothing rather than something unparseable.
    #[test]
    fn kube_node_info_v1_without_node_info() {
        assert!(KubeNodeInfoV1::from_node(&node("worker-1"), &host_settings()).is_none());
    }
}
