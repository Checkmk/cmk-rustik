//! Collection of Kubernetes entities from the kubelet stats cache.
//!
//! The domain half of the OTel module: everything that knows what a pod or
//! container is lives here, and new metrics get added here.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::kubernetes::TokenValidator;
use crate::ingest::kubelet_stats::{Container, Pod};
use crate::otel::wire::{Attribute, KubeEntity, KubeGauge, Value};
use crate::state::AppState;

/// Extract a container's samples as (memory working set bytes, CPU cores).
/// `None` where the kubelet reported no sample.
fn container_samples(container: &Container) -> (Option<i64>, Option<f64>) {
    let bytes = container
        .memory
        .as_ref()
        .and_then(|m| m.working_set_bytes)
        .map(|b| b as i64);
    let cores = container
        .cpu
        .as_ref()
        .and_then(|c| c.usage_nano_cores)
        .map(|n| n as f64 / 1e9);
    (bytes, cores)
}

/// The identity attributes shared by a pod and its containers.
fn pod_attributes(cluster: &str, node: &str, pod: &Pod) -> Vec<Arc<Attribute>> {
    vec![
        Arc::new(Attribute::new(
            "k8s.namespace.name",
            pod.pod_ref.namespace.clone(),
        )),
        Arc::new(Attribute::new("k8s.pod.name", pod.pod_ref.name.clone())),
        Arc::new(Attribute::new("k8s.node.name", node.to_string())),
        Arc::new(Attribute::new("k8s.cluster.name", cluster.to_string())),
    ]
}

fn epoch_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

/// Walk the kubelet stats cache and produce one [`KubeEntity`] per container
/// and one per pod (container sums).
///
/// A missing kubelet sample produces a *gap* rather than a fabricated zero:
/// the gauge is simply omitted, and an entity with no gauges at all is not
/// emitted (this mirrors the kubeletstats receiver's behavior).
pub(super) fn collect_entities(state: &AppState<impl TokenValidator>) -> Vec<KubeEntity> {
    let mut out: Vec<KubeEntity> = Vec::new();
    let now = epoch_nanos();
    let cluster = &state.host_settings.cluster_name;
    for (_, summary) in state.kubelet_stats_summary_cache.iter() {
        for pod in &summary.pods {
            // Attributes for the pod *and* its containers
            let p_attributes = pod_attributes(cluster, &summary.node.node_name, pod);

            // Pod aggregations. `None` until some container reports a sample.
            let mut p_working_set_bytes: Option<i64> = None;
            let mut p_usage_cores: Option<f64> = None;

            for container in &pod.containers {
                let (c_bytes, c_cores) = container_samples(container);
                let container_name =
                    Arc::new(Attribute::new("k8s.container.name", container.name.clone()));
                let mut gauges = Vec::new();
                if let Some(bytes) = c_bytes {
                    p_working_set_bytes = Some(p_working_set_bytes.unwrap_or(0) + bytes);
                    gauges.push(KubeGauge::new(
                        "container.memory.working_set",
                        "By",
                        Value::Bytes(bytes),
                        now,
                        vec![container_name.clone()],
                    ));
                }
                if let Some(cores) = c_cores {
                    p_usage_cores = Some(p_usage_cores.unwrap_or(0.0) + cores);
                    gauges.push(KubeGauge::new(
                        "container.cpu.usage",
                        "{cpu}",
                        Value::Cores(cores),
                        now,
                        vec![container_name.clone()],
                    ));
                }
                if gauges.is_empty() {
                    continue;
                }
                let mut c_attributes = p_attributes.to_vec();
                c_attributes.push(container_name);
                out.push(KubeEntity::new(c_attributes, gauges));
            }

            let mut gauges = Vec::new();
            if let Some(bytes) = p_working_set_bytes {
                gauges.push(KubeGauge::new(
                    "k8s.pod.memory.working_set",
                    "By",
                    Value::Bytes(bytes),
                    now,
                    Vec::new(),
                ));
            }
            if let Some(cores) = p_usage_cores {
                gauges.push(KubeGauge::new(
                    "k8s.pod.cpu.usage",
                    "{cpu}",
                    Value::Cores(cores),
                    now,
                    Vec::new(),
                ));
            }
            if !gauges.is_empty() {
                out.push(KubeEntity::new(p_attributes, gauges));
            }
        }
    }
    out
}
