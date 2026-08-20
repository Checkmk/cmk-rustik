use serde::Serialize;

use crate::host_settings::HostSettings;
use crate::ingest::api_health;
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

/// Cluster details. (`kube_cluster_details_v1`)
///
/// Provides API health (result of polling `/readyz` and `/livez` using the
/// Kubernetes API client).
#[derive(Serialize)]
pub(crate) struct KubeClusterDetailsV1<'a> {
    api_health: ApiHealth<'a>,
}

#[derive(Serialize)]
pub(crate) struct ApiHealth<'a> {
    live: ApiHealthResponse<'a>,
    ready: ApiHealthResponse<'a>,
}

#[derive(Serialize)]
pub(crate) struct ApiHealthResponse<'a> {
    status_code: u16,
    response: &'a str,
}

impl<'a> From<&'a api_health::HealthResponse> for ApiHealthResponse<'a> {
    fn from(response: &'a api_health::HealthResponse) -> ApiHealthResponse<'a> {
        Self {
            status_code: response.status_code,
            response: &response.body,
        }
    }
}

impl<'a> KubeClusterDetailsV1<'a> {
    pub fn new(update: &'a api_health::ApiHealthUpdate) -> Option<Self> {
        let Some(health) = update else {
            return None;
        };
        let live = &health.live;
        let ready = &health.ready;
        Some(Self {
            api_health: ApiHealth {
                live: live.into(),
                ready: ready.into(),
            },
        })
    }
}

impl Section for KubeClusterDetailsV1<'_> {
    const NAME: &'static str = "kube_cluster_details_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{host_settings, s};

    #[test]
    fn kube_cluster_info_v1() {
        insta::assert_json_snapshot!(KubeClusterInfoV1::from_host_settings(&host_settings()));
    }

    #[test]
    fn kube_cluster_details_v1() {
        let api_health = api_health::ApiHealth {
            live: api_health::HealthResponse {
                status_code: 200,
                body: s("ok"),
            },
            ready: api_health::HealthResponse {
                status_code: 200,
                body: s("ok"),
            },
        };
        insta::assert_json_snapshot!(KubeClusterDetailsV1::new(&Some(api_health.into())));
    }

    #[test]
    fn no_cluster_details_without_api_health() {
        assert!(KubeClusterDetailsV1::new(&None).is_none());
    }
}
