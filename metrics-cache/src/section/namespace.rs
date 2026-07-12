use k8s_openapi::api::core::v1::Namespace;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::host_settings::HostSettings;
use crate::section::Section;
use crate::section::common::LabelRef;

/// Namespace info. (`kube_namespace_info_v1`)
#[derive(Serialize)]
pub(crate) struct KubeNamespaceInfoV1<'a> {
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
    pub cluster: &'a str,
    pub kubernetes_cluster_hostname: &'a str,
}

impl<'a> KubeNamespaceInfoV1<'a> {
    pub fn from_namespace(
        namespace: &'a Namespace,
        settings: &'a HostSettings,
    ) -> Option<KubeNamespaceInfoV1<'a>> {
        let section = KubeNamespaceInfoV1 {
            name: namespace.metadata.name.as_deref()?,
            creation_timestamp: namespace
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.as_millisecond() as f64 / 1000.0),
            labels: namespace
                .metadata
                .labels
                .as_ref()
                .map(LabelRef::from_map)
                .unwrap_or_default(),
            annotations: namespace
                .metadata
                .annotations
                .as_ref()
                .map(|m| settings.annotation_key_pattern.filter(m))
                .unwrap_or_default(),
            cluster: &settings.cluster_name,
            kubernetes_cluster_hostname: &settings.cluster_host_name,
        };
        Some(section)
    }
}

impl Section for KubeNamespaceInfoV1<'_> {
    const NAME: &'static str = "kube_namespace_info_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    use crate::host_settings::AnnotationKeyPattern;
    use crate::test_support::{host_settings, namespace};

    #[test]
    fn kube_namespace_info_v1() {
        let namespace = namespace("my-ns");
        let mut settings = host_settings();
        insta::assert_json_snapshot!(KubeNamespaceInfoV1::from_namespace(&namespace, &settings));

        // This pattern should only match one annotation
        settings.annotation_key_pattern =
            AnnotationKeyPattern::Pattern(Regex::new("^example").unwrap());
        assert_eq!(
            KubeNamespaceInfoV1::from_namespace(&namespace, &settings)
                .unwrap()
                .annotations
                .len(),
            1
        );

        // Ignore all annotations, emit 0 of them
        settings.annotation_key_pattern = AnnotationKeyPattern::IgnoreAll;
        assert_eq!(
            KubeNamespaceInfoV1::from_namespace(&namespace, &settings)
                .unwrap()
                .annotations
                .len(),
            0
        );

        // Import all annotations (fixture default, captured by insta above, but let's be explicit)
        settings.annotation_key_pattern = AnnotationKeyPattern::ImportAll;
        assert_eq!(
            KubeNamespaceInfoV1::from_namespace(&namespace, &settings)
                .unwrap()
                .annotations
                .len(),
            2
        );
    }
}
