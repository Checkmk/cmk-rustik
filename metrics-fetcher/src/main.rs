use anyhow::Result;

fn main() -> Result<()> {
    // We use ring instead of aws-lc-rs
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    Ok(())
}
