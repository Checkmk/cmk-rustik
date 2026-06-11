mod error;
mod payload;

use anyhow::Result;

use crate::payload::Payload;

/// Query the Kubelet /stats/summary response and return the raw JSON payload.
///
/// We do not parse the payload or perform any calculations on it here, leaving
/// these tasks for metrics-cache to do.
pub(crate) async fn scrape_kubelet_stats_summary() -> error::Result<Payload> {
    let token = std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token")?;
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    let bytes = client
        .get("https://127.0.0.1:10250/stats/summary")
        .bearer_auth(token.trim())
        .send()
        .await?
        .bytes()
        .await?;
    Ok(Payload::KubeletStatsSummary(bytes))
}

#[tokio::main]
async fn main() -> Result<()> {
    // We use ring instead of aws-lc-rs
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let summary = scrape_kubelet_stats_summary().await?;
    println!("{:?}", summary);
    let resp = summary.push_to_metrics_cache().await?;
    println!("{resp:?}");
    Ok(())
}
