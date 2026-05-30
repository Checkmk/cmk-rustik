use axum::extract::State;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::snapshot::Snapshot;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> String {
    let snap = Snapshot::new(state.stores);
    let owner_ref_map = snap.map_object_uids_to_owner_ref();
    format!("{:?}", owner_ref_map)
}
