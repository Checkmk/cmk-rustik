use clap::Parser;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    version,
    name = "metrics-fetcher",
    about = "Fetch metrics from a Kubernetes node and send them to metrics-cache"
)]
pub struct CliArgs {
    /// Kubelet stats poll interval in seconds. Must be greater than zero.
    #[arg(long, default_value = "60", value_parser = parse_positive_seconds)]
    pub kubelet_stats_poll_interval: Duration,

    /// Kubelet health poll interval in seconds. Must be greater than zero.
    #[arg(long, default_value = "60", value_parser = parse_positive_seconds)]
    pub kubelet_health_poll_interval: Duration,

    /// System-agent poll interval in seconds. Must be greater than zero.
    #[arg(long, default_value = "60", value_parser = parse_positive_seconds)]
    pub system_agent_poll_interval: Duration,

    /// Timeout in seconds for each system-agent execution. Must be greater
    /// than zero.
    #[arg(long, default_value = "15", value_parser = parse_positive_seconds)]
    pub system_agent_timeout: Duration,

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

fn parse_positive_seconds(value: &str) -> Result<Duration, String> {
    let seconds = value
        .parse::<u64>()
        .map_err(|_| "expected a positive whole number of seconds".to_string())?;
    if seconds == 0 {
        return Err("must be greater than zero".to_string());
    }
    Ok(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scraper_timings_have_defaults_and_accept_overrides_in_seconds() {
        let defaults = CliArgs::try_parse_from(["metrics-fetcher"]).expect("valid defaults");
        assert_eq!(
            defaults.kubelet_stats_poll_interval,
            Duration::from_secs(60)
        );
        assert_eq!(
            defaults.kubelet_health_poll_interval,
            Duration::from_secs(60)
        );
        assert_eq!(defaults.system_agent_poll_interval, Duration::from_secs(60));
        assert_eq!(defaults.system_agent_timeout, Duration::from_secs(15));

        let configured = CliArgs::try_parse_from([
            "metrics-fetcher",
            "--kubelet-stats-poll-interval=30",
            "--kubelet-health-poll-interval=45",
            "--system-agent-poll-interval=90",
            "--system-agent-timeout=10",
        ])
        .expect("valid timing overrides");
        assert_eq!(
            configured.kubelet_stats_poll_interval,
            Duration::from_secs(30)
        );
        assert_eq!(
            configured.kubelet_health_poll_interval,
            Duration::from_secs(45)
        );
        assert_eq!(
            configured.system_agent_poll_interval,
            Duration::from_secs(90)
        );
        assert_eq!(configured.system_agent_timeout, Duration::from_secs(10));
    }

    #[test]
    fn scraper_timings_reject_zero_and_invalid_seconds() {
        for flag in [
            "--kubelet-stats-poll-interval",
            "--kubelet-health-poll-interval",
            "--system-agent-poll-interval",
            "--system-agent-timeout",
        ] {
            for value in ["0", "-1", "0.5", "invalid"] {
                let error =
                    CliArgs::try_parse_from(["metrics-fetcher", &format!("{flag}={value}")])
                        .expect_err("invalid timing must be rejected before starting scrapers");
                assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
            }
        }
    }
}
