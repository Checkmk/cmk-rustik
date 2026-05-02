use axum::{Json, extract::State};

use crate::AppState;
use crate::kube_auth::TokenValidator;
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
    // Add it to the cache
    state
        .machine_sections_cache
        .insert(
            machine_sections.sections.node_name.clone(),
            machine_sections.sections,
        )
        .await;
    Json("ok".to_string())
}
