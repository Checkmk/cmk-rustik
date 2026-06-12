use axum::{Json, extract::State};
use std::sync::Arc;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::kubelet_stats::StatsSummary;

pub async fn kubelet_stats_summary(
    State(state): State<AppState<impl TokenValidator>>,
    Json(stats_summary): Json<StatsSummary>,
) -> Json<String> {
    state
        .kubelet_stats_summary_cache
        .insert(
            stats_summary.node.node_name.clone(),
            Arc::new(stats_summary),
        )
        .await;
    Json("ok".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::assert_matches;

    const STATS_SUMMARY: &str =
        include_str!("../../tests/fixtures/ingress/kubelet_stats_summary.json");

    #[test]
    fn test_parses_kubelet_stats_summary() -> Result<()> {
        let parsed: StatsSummary = serde_json::from_str(STATS_SUMMARY)?;
        assert_eq!(parsed.pods.len(), 11);
        assert_eq!(parsed.node.node_name, "rustik-control-plane");
        assert_eq!(parsed.pods[3].pod_ref.name, "coredns-589f44dc88-48xcj");
        assert_eq!(parsed.pods[3].containers[0].name, "coredns");
        assert_matches!(
            parsed.pods[3].containers[0]
                .cpu
                .as_ref()
                .and_then(|c| c.usage_nano_cores),
            Some(3644771)
        );
        assert_matches!(
            parsed.pods[3].containers[0]
                .memory
                .as_ref()
                .and_then(|m| m.working_set_bytes),
            Some(15020032)
        );
        Ok(())
    }
}
