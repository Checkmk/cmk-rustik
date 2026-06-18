use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::piggyback::{PiggybackHost, pod::Pod};
use crate::section::writeable::frame;
use crate::snapshot::Snapshot;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Result<String, StatusCode> {
    let snap = Snapshot::new(state.stores, state.kubelet_stats_summary_cache);
    let sections: Vec<_> = snap
        .stores
        .pods
        .iter()
        .filter_map(|p| Pod::new(p, &snap))
        .flat_map(|host| host.emit())
        .filter_map(|r| match r {
            Ok(section) => Some(section),
            Err(e) => {
                tracing::warn!(section = %e.name, error = %e.source, "skipping section");
                None
            }
        })
        .collect();
    let mut out = Vec::new();
    frame(&mut out, sections).map_err(|e| {
        tracing::error!(%e, "framing failed writing to output vector");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}
