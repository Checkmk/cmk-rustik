mod cli_args;
mod error;
mod kubelet_stats_summary;
mod payload;
mod scraper;

use anyhow::Result;
use clap::Parser;
use std::io;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use crate::cli_args::CliArgs;
use crate::kubelet_stats_summary::KubeletStatsSummaryScraper;
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

    let kubelet_stats_summary_scraper = KubeletStatsSummaryScraper::new(Arc::new(args));
    let kubelet_scrape = tokio::spawn(kubelet_stats_summary_scraper.loop_push_scrape());
    let _ = tokio::try_join!(kubelet_scrape);
    Ok(())
}
