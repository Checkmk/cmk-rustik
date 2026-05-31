use axum::extract::State;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::snapshot::Snapshot;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> String {
    let snap = Snapshot::new(state.stores);
    format!("{:?}", snap)
}
