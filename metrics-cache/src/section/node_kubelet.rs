use k8s_openapi::api::core::v1::Node;
use serde::Serialize;

use crate::ingest::kubelet_health::KubeletHealth;
use crate::section::Section;
use crate::snapshot::kubelet_health::KubeletHealths;

/// Kubelet version and health. (`kube_node_kubelet_v1`)
#[derive(Serialize)]
pub(crate) struct KubeNodeKubeletV1<'a> {
    pub version: &'a str,
    pub health: &'a KubeletHealth,
}

impl<'a> KubeNodeKubeletV1<'a> {
    pub fn from_node(
        node: &'a Node,
        kubelet_healths: &'a KubeletHealths,
    ) -> Option<KubeNodeKubeletV1<'a>> {
        let node_info = node.status.as_ref()?.node_info.as_ref()?;
        let node_name = node.metadata.name.as_deref()?;
        let health = &kubelet_healths.get(node_name)?.payload;

        if let KubeletHealth::Response {
            status_code: 403, ..
        } = health
        {
            return None;
        }

        Some(KubeNodeKubeletV1 {
            version: &node_info.kubelet_version,
            health,
        })
    }
}

impl Section for KubeNodeKubeletV1<'_> {
    const NAME: &'static str = "kube_node_kubelet_v1";
}

#[cfg(test)]
mod tests {
    use moka::future::Cache;
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;

    use crate::ingest::MetricsFetcherIngestion;
    use crate::test_support::*;

    async fn kubelet_healths(entries: &[(&str, KubeletHealth)]) -> KubeletHealths {
        let cache = Cache::builder().build();
        for (name, health) in entries {
            let ingestion = MetricsFetcherIngestion {
                received_at: Instant::now(),
                metadata: Default::default(),
                payload: health.clone(),
            };
            cache.insert(name.to_string(), Arc::new(ingestion)).await;
        }
        cache.run_pending_tasks().await;
        KubeletHealths::from_cache(&cache)
    }

    #[tokio::test]
    async fn kube_node_kubelet_v1_response() {
        let node = node_prefilled("node01");
        let healths = kubelet_healths(&[(
            "node01",
            KubeletHealth::Response {
                status_code: 200,
                response: "ok".to_string(),
            },
        )])
        .await;
        insta::assert_json_snapshot!(KubeNodeKubeletV1::from_node(&node, &healths));
    }

    #[tokio::test]
    async fn kube_node_kubelet_v1_connection_error() {
        let node = node_prefilled("node01");
        let healths = kubelet_healths(&[(
            "node01",
            KubeletHealth::ConnectionError {
                message: "connection refused".to_string(),
            },
        )])
        .await;
        insta::assert_json_snapshot!(KubeNodeKubeletV1::from_node(&node, &healths));
    }

    #[tokio::test]
    async fn kube_node_kubelet_v1_no_health_reported() {
        let node = node_prefilled("node01");
        let healths = kubelet_healths(&[]).await;
        assert!(KubeNodeKubeletV1::from_node(&node, &healths).is_none());
    }

    #[tokio::test]
    async fn kube_node_kubelet_v1_restricted_node_proxy_permissions() {
        let node = node_prefilled("node01");
        let healths = kubelet_healths(&[(
            "node01",
            KubeletHealth::Response {
                status_code: 403,
                response: "Forbidden".to_string(),
            },
        )])
        .await;
        assert!(KubeNodeKubeletV1::from_node(&node, &healths).is_none());
    }
}
