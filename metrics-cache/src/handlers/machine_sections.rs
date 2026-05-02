use axum::Json;

use cmk_kube_types::machine_sections::MachineSections;

//pub async fn get() -> Json<HealthResponse> {
//    Json(HealthResponse {
//        status: "available".to_string(),
//    })
//}

pub async fn update(Json(machine_sections): Json<MachineSections>) -> Json<String> {
    let parsed = format!("{:?}", machine_sections);
    Json(parsed)
}
