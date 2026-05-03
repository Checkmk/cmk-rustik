use axum::{Json, extract::State};

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use cmk_kube_types::machine_sections::{FetchResult, MachineSections};

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Json<Vec<FetchResult>> {
    Json(
        state
            .machine_sections_cache
            .iter()
            .map(|(_, v)| v)
            .collect(),
    )
}

pub async fn update(
    State(state): State<AppState<impl TokenValidator>>,
    Json(machine_sections): Json<MachineSections>,
) -> Json<String> {
    let metadata_key = format!(
        "machine_sections:{}",
        machine_sections.metadata.static_metadata.node
    );
    // Add it to the cache
    state
        .machine_sections_cache
        .insert(
            machine_sections.sections.node_name.clone(),
            machine_sections.sections,
        )
        .await;
    // And its metadata
    state
        .metrics_fetcher_metadata_cache
        .insert(metadata_key, machine_sections.metadata)
        .await;
    Json("ok".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::test_app_state;
    use axum::Json;
    use cmk_kube_types::machine_sections::MachineSections;
    use cmk_kube_types::metadata::metrics_fetcher::FetcherKind;

    fn make_machine_sections(node: &str) -> MachineSections {
        MachineSections {
            sections: cmk_kube_types::machine_sections::FetchResult {
                node_name: node.to_string(),
                sections: "s".to_string(),
            },
            metadata: cmk_kube_types::metadata::metrics_fetcher::Metadata {
                static_metadata: cmk_kube_types::metadata::StaticMetadata {
                    node: node.to_string(),
                    host_name: "host".to_string(),
                    container_platform: cmk_kube_types::metadata::Platform {
                        os_name: "linux".to_string(),
                        os_version: "1".to_string(),
                        python_version: String::new(),
                        python_compiler: String::new(),
                    },
                    checkmk_kube_agent: cmk_kube_types::metadata::CheckmkKubeAgent {
                        project_version: "v0".to_string(),
                    },
                },
                collector_type: FetcherKind::Machine,
                components: Default::default(),
            },
        }
    }

    #[tokio::test]
    async fn update_inserts_into_caches() {
        let state = test_app_state();
        let machine_sections = make_machine_sections("node-x");

        let resp = update(State(state.clone()), Json(machine_sections.clone())).await;
        assert_eq!(resp.0, "ok");

        // check machine_sections_cache
        let got = state
            .machine_sections_cache
            .get(&machine_sections.sections.node_name)
            .await
            .expect("missing");
        assert_eq!(got.node_name, machine_sections.sections.node_name);

        // check metadata cache
        let metadata_key = format!(
            "machine_sections:{}",
            machine_sections.metadata.static_metadata.node
        );
        let meta = state
            .metrics_fetcher_metadata_cache
            .get(&metadata_key)
            .await
            .expect("missing meta");
        assert_eq!(
            meta.static_metadata.node,
            machine_sections.metadata.static_metadata.node
        );
    }

    #[tokio::test]
    async fn get_returns_cached_entries() {
        let state = test_app_state();
        let fr = cmk_kube_types::machine_sections::FetchResult {
            node_name: "node-a".to_string(),
            sections: "s".to_string(),
        };
        state
            .machine_sections_cache
            .insert(fr.node_name.clone(), fr.clone())
            .await;

        let resp = get(State(state)).await;
        let vec = resp.0;
        assert!(vec.iter().any(|f| f.node_name == fr.node_name));
    }
}
