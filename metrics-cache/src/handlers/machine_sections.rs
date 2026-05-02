use axum::{Json, extract::State};

use crate::AppState;
use crate::kube_auth::TokenValidator;
use cmk_kube_types::machine_sections::MachineSections;

//pub async fn get() -> Json<HealthResponse> {
//    Json(HealthResponse {
//        status: "available".to_string(),
//    })
//}

pub async fn update<V: TokenValidator>(
    State(state): State<AppState<V>>,
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
