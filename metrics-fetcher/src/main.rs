mod prometheus_parser;

use anyhow::Result;

use crate::prometheus_parser::Sample;
use cmk_kube_types::container_metrics::Metric;

fn sample_to_metric(sample: Sample, default_timestamp_ms: i64) -> Option<Metric> {
    fn non_empty(s: Option<&String>) -> Option<String> {
        s.filter(|v| !v.is_empty()).cloned()
    }

    let container_name = non_empty(sample.labels.get("name"))?;
    let pod_uid = non_empty(sample.labels.get("container_label_io_kubernetes_pod_uid"))?;
    let namespace = sample
        .labels
        .get("container_label_io_kubernetes_pod_namespace")?
        .clone();
    let pod_name = sample
        .labels
        .get("container_label_io_kubernetes_pod_name")?
        .clone();
    let ts_ms = sample
        .timestamp
        .as_deref()
        .and_then(|t| t.parse().ok())
        .unwrap_or(default_timestamp_ms);
    Some(Metric {
        container_name,
        namespace,
        pod_uid,
        pod_name,
        metric_name: sample.metric_name,
        metric_value_string: sample.value,
        timestamp: ts_ms as f64 / 1000.0,
    })
}

fn samples_to_metrics(samples: Vec<Sample>, default_timestamp_ms: i64) -> Vec<Metric> {
    samples
        .into_iter()
        .filter_map(|s| sample_to_metric(s, default_timestamp_ms))
        .collect()
}

fn fetch_cadvisor_metrics() -> Result<Vec<Metric>> {
    let body = reqwest::blocking::get("http://localhost:8080/metrics")?.text()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    prometheus_parser::exposition(&body)
        .map_err(|e| anyhow::anyhow!("Failed to parse Prometheus metrics: {}", e))
        .map(|(_, samples)| samples_to_metrics(samples, now_ms))
}

fn main() -> Result<()> {
    // We use ring instead of aws-lc-rs
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let metrics = fetch_cadvisor_metrics()?;
    for metric in metrics {
        println!("{:?}", metric);
    }
    Ok(())
}
