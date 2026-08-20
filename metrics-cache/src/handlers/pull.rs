use axum::extract::State;
use axum::http::StatusCode;
use bytes::Bytes;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::piggyback::emit_all;
use crate::section::writeable::frame;
use crate::snapshot::Snapshot;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Result<Bytes, StatusCode> {
    let snap = Snapshot::new(
        state.stores,
        state.kubelet_stats_summary_cache,
        state.kubelet_health_cache,
        state.system_agent_cache,
    );
    let sections = emit_all(&snap, &state.host_settings);
    let mut out = Vec::new();
    frame(&mut out, sections).map_err(|e| {
        tracing::error!(%e, "framing failed writing to output vector");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(out.into())
}
