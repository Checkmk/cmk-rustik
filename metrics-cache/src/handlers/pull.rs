use axum::extract::State;
use axum::http::StatusCode;
use bytes::Bytes;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::piggyback::emit_all;
use crate::section::writeable::frame;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Result<Bytes, StatusCode> {
    let snap = state.snapshot();
    let sections = emit_all(&snap, &state.host_settings);
    let mut out = Vec::new();
    frame(&mut out, sections).map_err(|e| {
        tracing::error!(%e, "framing failed writing to output vector");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(out.into())
}
