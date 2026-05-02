use clap::Parser;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    version,
    name = "cmk-rustik-cache-server",
    about = "An HTTP-based caching server for Kubernetes metrics"
)]
pub struct Args {
    /// Path to the SSL key file for HTTPS connections
    #[arg(short = 'k', long = "ssl-keyfile", requires = "secure_protocol")]
    pub ssl_keyfile: Option<String>,

    /// Path to the SSL certificate file for HTTPS connections
    #[arg(short = 'c', long = "ssl-certfile", requires = "secure_protocol")]
    pub ssl_certfile: Option<String>,

    /// Use secure protocol (HTTPS)
    #[arg(short = 'S', long = "secure-protocol", default_value_t = false)]
    pub secure_protocol: bool,

    /// Service accounts that have access to query data from the metrics cache
    /// API GET endpoints. Comma-separated, in the form NAMESPACE:SERVICEACCOUNT
    #[arg(
        long = "reader-allowlist",
        alias = "reader-whitelist",
        value_delimiter = ',',
        default_value = "checkmk-monitoring:checkmk"
    )]
    pub reader_allowlist: Vec<String>,

    /// Service accounts that have access to query data from the metrics cache
    /// API POST endpoints. Comma-separated, in the form
    /// NAMESPACE:SERVICEACCOUNT
    #[arg(
        long = "writer-allowlist",
        alias = "writer-whitelist",
        value_delimiter = ',',
        default_value = "checkmk-monitoring:node-collector" // Backwards compat
    )]
    pub writer_allowlist: Vec<String>,

    /// Maximum number of metric entries the metrics cache can hold before
    /// entries start being discarded
    #[arg(short = 'm', long = "cache-maxsize", default_value_t = 10000)]
    pub cache_maxsize: u32,

    /// How long (seconds) entries are persisted in the cache
    #[arg(
        short = 't',
        long = "cache-ttl",
        value_parser = parse_duration,
        default_value = "120"
    )]
    pub cache_ttl: Duration,

    /// How verbose to log
    #[arg(
        short = 'l',
        long = "log-level",
        value_parser = ["debug", "info", "warning", "error", "critical"],
        default_value = "error",
    )]
    pub log_level: String,

    /// IP address to bind to
    #[arg(short = 's', long = "address", default_value = "127.0.0.1")]
    pub address: String,

    /// Port to bind to
    #[arg(short = 'p', long = "port", default_value_t = 10050)]
    pub port: u16,

    /// Maximum number of retries when contacting the Kubernetes API for
    /// authentication
    #[arg(short = 'r', long = "max-retries", default_value_t = 3)]
    pub max_retries: u8,

    /// Time in seconds to wait for a TCP connection to the Kubernetes API
    /// during authentication
    #[arg(
        short = 'C',
        long = "connect-timeout",
        value_parser = parse_duration,
        default_value = "10",
    )]
    pub connect_timeout: Duration,

    /// Time in seconds to wait for a response from the Kubernetes API during
    /// authentication
    #[arg(
        short = 'R',
        long = "read-timeout",
        value_parser = parse_duration,
        default_value = "12",
    )]
    pub read_timeout: Duration,
}

/// Convert a numeric argument given by the user as seconds into a Duration.
fn parse_duration(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}
