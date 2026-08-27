//! System (Checkmk) agent ingress types/helpers

use axum::body::Bytes;

/// Raw output from a machine-level agent (currently only Linux's
/// `check_mk_agent`, but named generically since a Windows agent could push
/// here too some day). A distinct type rather than a bare [`Bytes`], so the
/// cache's shape stays unambiguous if another `Bytes`-based payload is ever
/// added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemAgentOutput(pub Bytes);
