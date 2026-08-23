use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    version,
    name = "metrics-fetcher",
    about = "Fetch metrics from a Kubernetes node and send them to metrics-cache"
)]
pub struct CliArgs {
    /// Namespace that metrics-cache lives in (used for constructing the URL to
    /// send metrics to).
    #[arg(long, default_value = "checkmk-monitoring")]
    pub metrics_cache_namespace: String,

    /// Service name that metrics-cache responds on (used for constructing the
    /// URL to send metrics to).
    #[arg(long, default_value = "cmk-rustik-metrics-cache")]
    pub metrics_cache_service: String,

    /// Port to talk to metrics-cache on (corresponds to the service specified
    /// via --metrics-cache-service).
    #[arg(long, default_value_t = 10050)]
    pub metrics_cache_port: u16,

    /// CA Certificate for connecting to metrics-cache. When not specified, HTTP is used.
    #[arg(long)]
    pub metrics_cache_ca_cert_file: Option<String>,
}
