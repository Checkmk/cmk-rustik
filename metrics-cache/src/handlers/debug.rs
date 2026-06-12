use axum::extract::State;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::piggyback_host::{PiggybackHost, Pod};
use crate::snapshot::Snapshot;
use crate::writeable_section::frame;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> String {
    let snap = Snapshot::new(state.stores, state.kubelet_stats_summary_cache);
    let sections: Vec<_> = snap
        .stores
        .pods
        .iter()
        .filter_map(|p| Pod::new(p, &snap))
        .flat_map(|host| host.emit(&snap))
        .filter_map(|r| match r {
            Ok(section) => Some(section),
            Err(e) => {
                tracing::warn!(section = %e.name, error = %e.source, "skipping section");
                None
            }
        })
        .collect();
    frame(sections)
}
