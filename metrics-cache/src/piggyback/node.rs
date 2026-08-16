use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::sync::Arc;

use crate::host_settings::HostSettings;
use crate::piggyback::{AggregationHost, Meta, PiggybackHost};
use crate::section::node::KubeNodeInfoV1;
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct Node<'a> {
    api: &'a v1::Node,
    meta: Meta<'a>,
    snapshot: &'a Snapshot,
    settings: &'a HostSettings,
}

impl Node<'_> {
    pub fn new<'a>(
        api: &'a v1::Node,
        snapshot: &'a Snapshot,
        settings: &'a HostSettings,
    ) -> Option<Node<'a>> {
        let meta = Meta::from_resource(api)?;
        Some(Node {
            api,
            meta,
            snapshot,
            settings,
        })
    }
}

impl AggregationHost for Node<'_> {
    fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    fn pods(&self) -> impl Iterator<Item = &Arc<v1::Pod>> {
        self.snapshot.indexes.pods_by_node(self.meta.name).iter()
    }
}

impl PiggybackHost for Node<'_> {
    fn metadata(&self) -> Option<&ObjectMeta> {
        Some(&self.api.metadata)
    }

    fn kind(&self) -> &str {
        &self.meta.kind
    }

    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        let me = self.meta.piggyback_hostname(&self.settings.cluster_name);
        let mut out = Vec::new();
        if let Some(kube_node_info_v1) = KubeNodeInfoV1::from_node(self.api, self.settings) {
            out.push(WriteableSection::of(&me, &kube_node_info_v1));
        }
        if let Some(ingestion) = self.snapshot.system_agent_snapshot.get(self.meta.name) {
            out.push(Ok(WriteableSection::raw(&me, ingestion.payload.0.clone())));
        }
        out.extend(self.aggregation_sections(&me));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::time::Instant;

    use crate::ingest::{MetricsFetcherIngestion, SystemAgentOutput};
    use crate::section::writeable::SectionBody;
    use crate::state::tests::test_app_state;
    use crate::test_support;

    #[tokio::test]
    async fn emit_includes_raw_system_agent_output_keyed_by_bare_node_name() {
        let state = test_app_state();
        let cache = state.system_agent_cache.clone();
        cache
            .insert(
                "node-1".to_string(),
                Arc::new(MetricsFetcherIngestion {
                    received_at: Instant::now(),
                    payload: SystemAgentOutput(Bytes::from_static(b"<<<check_mk>>>\n")),
                }),
            )
            .await;
        cache.run_pending_tasks().await;

        let api = test_support::node("node-1");
        let host_settings = state.host_settings.clone();
        let snapshot = Snapshot::new(
            state.stores,
            state.kubelet_stats_summary_cache,
            state.system_agent_cache,
        );
        let node = Node::new(&api, &snapshot, &host_settings).unwrap();

        let raw_section = node
            .emit()
            .into_iter()
            .filter_map(Result::ok)
            .find(|s| matches!(s.body, SectionBody::Raw(_)))
            .expect("expected a raw system agent section");

        assert_eq!(raw_section.piggyback_hostname, "node_testcluster_node-1");
        match raw_section.body {
            SectionBody::Raw(raw) => assert_eq!(raw, Bytes::from_static(b"<<<check_mk>>>\n")),
            SectionBody::Json { .. } => unreachable!(),
        }
    }
}
