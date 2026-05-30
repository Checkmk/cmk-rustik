use kube::Client;
use std::time::Duration;

/// The various Kinds with which we concern ourselves.
/// This is used in constructing an [`OwnerGraph`].
#[derive(Debug)]
pub enum Kind {
    Pod,
    Node,
    Deployment,
    DaemonSet,
    Namespace,
    ReplicaSet,
}

/// The unique ID of a given Kubernetes object.
#[derive(Hash, Eq, PartialEq, Debug)]
pub struct Uid(pub String); // TODO: Debatable if this should be Deref or not.

/// Build a Kubernetes client for general use (token reviews, etc.).
pub async fn client(
    connect_timeout: Duration,
    read_timeout: Duration,
) -> Result<Client, crate::error::Error> {
    let mut config = kube::Config::infer().await?;
    config.connect_timeout = Some(connect_timeout);
    config.read_timeout = Some(read_timeout);

    Ok(Client::try_from(config)?)
}

/// Build a Kubernetes client suitable for watch streams. No read timeout —
/// watch connections are long-lived and idle between events, so a read timeout
/// would kill them.
pub async fn watcher_client(connect_timeout: Duration) -> Result<Client, crate::error::Error> {
    let mut config = kube::Config::infer().await?;
    config.connect_timeout = Some(connect_timeout);

    Ok(Client::try_from(config)?)
}
