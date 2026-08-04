use clap::Parser;
use regex::Regex;
use std::time::Duration;

pub struct TlsConfig {
    pub secret_name: Option<String>,
    pub generate_if_missing: bool,
    pub namespace: Option<String>,
    pub service_name: Option<String>,
    pub longevity: Duration,
}

#[derive(Debug, Parser)]
#[command(
    version,
    name = "cmk-rustik-cache-server",
    about = "An HTTP-based caching server for Kubernetes metrics"
)]
pub struct CliArgs {
    /// The namespace metrics-cache is running in
    #[arg(long, env = "NAMESPACE")]
    pub namespace: Option<String>,

    /// IP address to bind to for intra-cluster ingest
    #[arg(long, default_value = "127.0.0.1")]
    pub ingest_address: String,

    /// Port to bind to for intra-cluster ingest
    #[arg(long, default_value_t = 10049)]
    pub ingest_port: u16,

    /// The service name for intra-cluster ingest
    #[arg(long)]
    pub ingest_service_name: Option<String>,

    /// Secret containing the TLS certificate and key for intra-cluster ingest
    #[arg(long)]
    pub ingest_tls_secret: Option<String>,

    /// Generate the intra-cluster ingest TLS CA and certificate if missing and
    /// store it as a Kubernetes Secret
    #[arg(
        long,
        requires_all = ["namespace", "ingest_service_name", "ingest_tls_secret"],
        default_value_t = false
    )]
    pub ingest_tls_secret_generate_if_missing: bool,

    /// When generating the intra-cluster ingest TLS CA and certificate,
    /// specifies how long they should be valid for in days
    #[arg(
        long,
        requires = "ingest_tls_secret_generate_if_missing",
        default_value = "3650",
        value_parser = parse_duration_days,
    )]
    pub ingest_tls_secret_generation_validity: Duration,

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
        value_parser = parse_duration_secs,
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

    /// IP address to bind to for pull mode
    #[arg(long, default_value = "127.0.0.1")]
    pub pull_address: String,

    /// Port to bind to for pull mode
    #[arg(long, default_value_t = 10050)]
    pub pull_port: u16,

    /// The service name for pull mode
    #[arg(long)]
    pub pull_service_name: Option<String>,

    /// Secret containing the TLS certificate and key for pull mode
    #[arg(long)]
    pub pull_tls_secret: Option<String>,

    /// Generate the pull mode TLS CA and certificate if missing and store it as
    /// a Kubernetes Secret
    #[arg(
        long,
        requires_all = ["namespace", "pull_service_name", "pull_tls_secret"],
        default_value_t = false
    )]
    pub pull_tls_secret_generate_if_missing: bool,

    /// When generating the pull-mode  TLS CA and certificate, specifies how
    /// long they should be valid for in days
    #[arg(
        long,
        requires = "pull_tls_secret_generate_if_missing",
        default_value = "3650",
        value_parser = parse_duration_days,
    )]
    pub pull_tls_secret_generation_validity: Duration,

    /// Maximum number of retries when contacting the Kubernetes API for
    /// authentication
    #[arg(short = 'r', long = "max-retries", default_value_t = 3)]
    pub max_retries: u8,

    /// Time in seconds to wait for a TCP connection to the Kubernetes API
    /// during authentication
    #[arg(
        short = 'C',
        long = "connect-timeout",
        value_parser = parse_duration_secs,
        default_value = "10",
    )]
    pub connect_timeout: Duration,

    /// Time in seconds to wait for a response from the Kubernetes API during
    /// authentication
    #[arg(
        short = 'R',
        long = "read-timeout",
        value_parser = parse_duration_secs,
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

    /// Make the pull-based endpoints publicly accessible
    #[arg(long, default_value_t = false)]
    pub disable_pull_authentication: bool,

    /// For pull mode, the shared secret that is also stored in Checkmk's
    /// password store. If not set, pull mode is disabled and requests will give
    /// a 401. To disable auth, use --disable-pull-authentication
    #[arg(long, env = "CMK_PULL_SHARED_SECRET", hide_env_values = true)]
    pub pull_shared_secret: Option<String>,

    /// Enable push mode and send sections to the specified server (including
    /// port)
    #[arg(long = "push-receiver")]
    pub push_receiver: Option<String>,

    /// Token to register with the Checkmk push-agent receiver for push mode
    #[arg(long, env = "CMK_PUSH_AGENT_RECEIVER_OTT", hide_env_values = true)]
    pub push_registration_ott: Option<String>,

    /// CA certificate (PEM) of the Checkmk site, used to verify we are about to
    /// register with the correct push-agent receiver
    #[arg(
        long,
        env = "CMK_PUSH_AGENT_RECEIVER_SITE_CA_PEM",
        hide_env_values = true
    )]
    pub push_registration_pem: Option<String>,

    /// Avoid verifying the identity of the configured push-agent receiver
    /// during initial registration. Do NOT use in production.
    #[arg(long, default_value_t = false)]
    pub push_registration_insecure_skip_site_ca_verification: bool,

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

    /// Emit all CronJob resources rather than only annotated ones
    #[arg(long = "all-cronjobs")]
    pub all_cronjobs: bool,
}

impl CliArgs {
    pub fn ingest_tls_config(&self) -> TlsConfig {
        TlsConfig {
            secret_name: self.ingest_tls_secret.clone(),
            generate_if_missing: self.ingest_tls_secret_generate_if_missing,
            namespace: self.namespace.clone(),
            service_name: self.ingest_service_name.clone(),
            longevity: self.ingest_tls_secret_generation_validity,
        }
    }

    pub fn pull_tls_config(&self) -> TlsConfig {
        TlsConfig {
            secret_name: self.pull_tls_secret.clone(),
            generate_if_missing: self.pull_tls_secret_generate_if_missing,
            namespace: self.namespace.clone(),
            service_name: self.pull_service_name.clone(),
            longevity: self.pull_tls_secret_generation_validity,
        }
    }
}

/// Convert a numeric argument given by the user as seconds into a Duration.
fn parse_duration_secs(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}

/// Convert a numeric argument given by the user as days into a Duration.
fn parse_duration_days(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let days: u64 = arg.parse()?;
    Ok(Duration::from_secs(60 * 60 * 24 * days))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse(extra_args: &[&str]) -> Result<CliArgs, clap::Error> {
        let mut args = vec![
            "metrics-cache",
            "--cluster-name",
            "cluster",
            "--cluster-host-name",
            "cluster-host",
        ];
        args.extend_from_slice(extra_args);
        CliArgs::try_parse_from(args)
    }

    #[test]
    fn existing_tls_secrets_do_not_require_generation_arguments() {
        for secret_arg in ["--pull-tls-secret", "--ingest-tls-secret"] {
            assert!(parse(&[secret_arg, "existing-tls"]).is_ok());
        }
    }

    #[test]
    fn generated_tls_secret_configuration_parses() {
        for (secret_arg, generate_arg, service_arg) in [
            (
                "--pull-tls-secret",
                "--pull-tls-secret-generate-if-missing",
                "--pull-service-name",
            ),
            (
                "--ingest-tls-secret",
                "--ingest-tls-secret-generate-if-missing",
                "--ingest-service-name",
            ),
        ] {
            assert!(
                parse(&[
                    "--namespace",
                    "monitoring",
                    secret_arg,
                    "generated-tls",
                    service_arg,
                    "metrics-cache",
                    generate_arg,
                ])
                .is_ok()
            );
        }
    }

    #[test]
    fn generated_tls_secret_requires_secret_and_service_names() {
        for args in [
            vec![
                "--namespace",
                "monitoring",
                "--pull-service-name",
                "metrics-cache",
                "--pull-tls-secret-generate-if-missing",
            ],
            vec![
                "--namespace",
                "monitoring",
                "--pull-tls-secret",
                "generated-tls",
                "--pull-tls-secret-generate-if-missing",
            ],
            vec![
                "--namespace",
                "monitoring",
                "--ingest-service-name",
                "metrics-cache",
                "--ingest-tls-secret-generate-if-missing",
            ],
            vec![
                "--namespace",
                "monitoring",
                "--ingest-tls-secret",
                "generated-tls",
                "--ingest-tls-secret-generate-if-missing",
            ],
        ] {
            assert_eq!(
                parse(&args)
                    .expect_err("incomplete TLS generation configuration should fail")
                    .kind(),
                ErrorKind::MissingRequiredArgument
            );
        }
    }
}
