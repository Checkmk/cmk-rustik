use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;
use crate::piggyback::{PiggybackHost, namespace::Namespace, pod::Pod};
use crate::section::writeable::{WriteableSection, frame};
use crate::snapshot::Snapshot;

pub async fn get(State(state): State<AppState<impl TokenValidator>>) -> Result<String, StatusCode> {
    let snap = Snapshot::new(state.stores, state.kubelet_stats_summary_cache);
    let pod_sections: Vec<WriteableSection> = snap
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

    let namespace_sections: Vec<_> = snap
        .stores
        .namespaces
        .iter()
        .filter_map(|n| Namespace::new(n, &snap))
        .flat_map(|host| host.emit())
        .filter_map(|r| match r {
            Ok(section) => Some(section),
            Err(e) => {
                tracing::warn!(section = %e.name, error = %e.source, "skipping section");
                None
            }
        })
        .collect();

    let mut sections: Vec<_> = pod_sections;
    sections.extend(namespace_sections);

    let mut out = Vec::new();
    frame(&mut out, sections).map_err(|e| {
        tracing::error!(%e, "framing failed writing to output vector");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}
