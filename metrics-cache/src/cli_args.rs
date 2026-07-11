use clap::Parser;
use regex::Regex;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    version,
    name = "cmk-rustik-cache-server",
    about = "An HTTP-based caching server for Kubernetes metrics"
)]
pub struct CliArgs {
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
        value_parser = ["trace", "debug", "info", "warn", "error", "off"],
        default_value = "info",
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

    /// The name of the cluster, included in each piggyback hostname
    #[arg(long = "cluster-name")]
    pub cluster_name: String,

    /// The name of the source host in Checkmk (should be an exact match)
    #[arg(long = "cluster-host-name")]
    pub cluster_host_name: String,

    /// Import all annotations as host labels
    #[arg(long = "import-all-annotations", default_value_t = false)]
    pub import_all_annotations: bool,

    #[arg(
        long = "annotation-key-pattern",
        conflicts_with = "import_all_annotations"
    )]
    pub annotation_key_pattern: Option<Regex>,

    /// Enable push mode and send sections to the specified server (including
    /// port)
    #[arg(long = "push-receiver")]
    pub push_receiver: Option<String>,

    /// Excluded node role (infix) patterns for cluster-level aggregations,
    /// comma-separated
    #[arg(long = "excluded-node-role-patterns", value_delimiter = ',')]
    pub excluded_node_role_patterns: Vec<Regex>,

    /// Enable sending OTel metrics to the endpoint given
    #[arg(long = "otel-endpoint")]
    pub otel_endpoint: Option<String>,

    /// Emit all Pod resources rather than only annotated ones
    #[arg(long = "all-pods")]
    pub all_pods: bool,

    /// Emit all Namespace resources rather than only annotated ones
    #[arg(long = "all-namespaces")]
    pub all_namespaces: bool,

    /// Emit all Node resources rather than only annotated ones
    #[arg(long = "all-nodes")]
    pub all_nodes: bool,

    /// Emit all Deployment resources rather than only annotated ones
    #[arg(long = "all-deployments")]
    pub all_deployments: bool,

    /// Emit all DaemonSet resources rather than only annotated ones
    #[arg(long = "all-daemonsets")]
    pub all_daemonsets: bool,

    /// Emit all StatefulSet resources rather than only annotated ones
    #[arg(long = "all-statefulsets")]
    pub all_statefulsets: bool,
}

/// Convert a numeric argument given by the user as seconds into a Duration.
fn parse_duration(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}
