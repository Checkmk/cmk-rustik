use kube::Client;
use std::time::Duration;

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
