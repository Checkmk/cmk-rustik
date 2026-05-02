use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "available".to_string(),
    })
}
