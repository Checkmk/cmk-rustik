use serde::Serialize;

use crate::host_settings::HostSettings;
use crate::section::Section;

/// Cluster info. (`kube_cluster_info_v1`)
#[derive(Serialize)]
pub(crate) struct KubeClusterInfoV1<'a> {
    pub name: &'a str,
    pub version: &'a str,
}

impl<'a> KubeClusterInfoV1<'a> {
    pub(crate) fn from_host_settings(settings: &'a HostSettings) -> KubeClusterInfoV1<'a> {
        KubeClusterInfoV1 {
            name: settings.cluster_name.as_str(),
            version: settings.cluster_version.as_str(),
        }
    }
}

impl Section for KubeClusterInfoV1<'_> {
    const NAME: &'static str = "kube_cluster_info_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::host_settings;

    #[test]
    fn kube_cluster_info_v1() {
        insta::assert_json_snapshot!(KubeClusterInfoV1::from_host_settings(&host_settings()));
    }
}
