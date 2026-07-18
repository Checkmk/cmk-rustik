pub mod common;
pub mod namespace;
pub mod performance;
pub mod pod;
pub mod pvc;
pub mod resource;
pub mod self_health;
pub mod writeable;

use serde::Serialize;

/// Wire-protocol JSON to send to Checkmk.
///
/// The types that implement this are relatively "low level", i.e. they sit
/// "close" to the wire protocol that they end up serializing into.
pub trait Section: Serialize {
    const NAME: &str;
}
