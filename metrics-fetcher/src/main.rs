mod cli_args;
mod error;
mod kubelet_stats_summary;
mod linux_agent;
mod payload;
mod scraper;

use anyhow::Result;
use clap::Parser;
use reqwest::ClientBuilder;
use std::io;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use crate::cli_args::CliArgs;
use crate::kubelet_stats_summary::KubeletStatsSummaryScraper;
use crate::linux_agent::LinuxAgentScraper;
use crate::scraper::Scraper;

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_writer(io::stderr)
        .init();

    // We use ring instead of aws-lc-rs
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Client to communicate with metrics cache; we allocate it just once, up front,
    // and share it between scrapers.
    let metrics_cache_client = match args.metrics_cache_ca_cert_file.as_deref() {
        Some(file) => {
            let pem = tokio::fs::read(file).await?;
            let ca = reqwest::Certificate::from_pem(&pem)?;
            ClientBuilder::new().tls_certs_only([ca])
        }
        None => ClientBuilder::new(),
    }
    .build()?;

    let args = Arc::new(args);
    let kubelet_stats_summary_scraper =
        KubeletStatsSummaryScraper::new(args.clone(), metrics_cache_client.clone());
    let linux_agent_scraper = LinuxAgentScraper::new(args.clone(), metrics_cache_client);
    let kubelet_scrape = tokio::spawn(kubelet_stats_summary_scraper.loop_push_scrape());
    let linux_agent_scrape = tokio::spawn(linux_agent_scraper.loop_push_scrape());

    tokio::select! {
        res = kubelet_scrape => {
            tracing::error!(error = ?res, "kubelet stats summary scrape loop exited unexpectedly");
            anyhow::bail!("kubelet stats summary scrape loop terminated unexpectedly");
        }
        res = linux_agent_scrape => {
            tracing::error!(error = ?res, "linux agent scrape loop exited unexpectedly");
            anyhow::bail!("linux agent scrape loop terminated unexpectedly");
        }
    }
}
