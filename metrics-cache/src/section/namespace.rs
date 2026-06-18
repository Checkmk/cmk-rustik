use serde::Serialize;
use std::collections::BTreeMap;

use crate::section::{Section, common::LabelRef};

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

impl Section for KubeNamespaceInfoV1<'_> {
    const NAME: &'static str = "kube_namespace_info_v1";
}
