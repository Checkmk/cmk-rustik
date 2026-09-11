//! Collection of Kubernetes entities from the kubelet stats cache.
//!
//! The domain half of the OTel module: everything that knows what a pod or
//! container is lives here, and new metrics get added here.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ingest::MetricsFetcherIngestion;
use crate::ingest::kubelet_stats::{Container, Pod, StatsSummary};
use crate::otel::wire::{Attribute, KubeEntity, KubeGauge, Value};
use crate::snapshot::owner_graph::OwnerGraph;

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

fn owner_attributes(owner_graph: &OwnerGraph, pod_uid: &str) -> Vec<Arc<Attribute>> {
    owner_graph
        .walk_up(pod_uid)
        .into_iter()
        .flat_map(|owner| match owner.kind.as_str() {
            "ReplicaSet" => vec![Attribute::new("k8s.replicaset.name", owner.name.clone())],
            "Deployment" => vec![Attribute::new("k8s.deployment.name", owner.name.clone())],
            "StatefulSet" => vec![Attribute::new("k8s.statefulset.name", owner.name.clone())],
            "DaemonSet" => vec![Attribute::new("k8s.daemonset.name", owner.name.clone())],
            "Job" => vec![Attribute::new("k8s.job.name", owner.name.clone())],
            "CronJob" => vec![Attribute::new("k8s.cronjob.name", owner.name.clone())],
            _ => vec![
                Attribute::new("k8s.owner.kind", owner.kind.clone()),
                Attribute::new("k8s.owner.name", owner.name.clone()),
            ],
        })
        .map(Arc::new)
        .collect()
}

/// The identity attributes shared by a pod and its containers.
fn pod_attributes(
    cluster: &str,
    node: &str,
    pod: &Pod,
    owner_graph: &OwnerGraph,
) -> Vec<Arc<Attribute>> {
    let mut attributes = vec![
        Arc::new(Attribute::new(
            "k8s.namespace.name",
            pod.pod_ref.namespace.clone(),
        )),
        Arc::new(Attribute::new("k8s.pod.name", pod.pod_ref.name.clone())),
        Arc::new(Attribute::new("k8s.node.name", node.to_string())),
        Arc::new(Attribute::new("k8s.cluster.name", cluster.to_string())),
    ];
    attributes.extend(owner_attributes(owner_graph, &pod.pod_ref.uid));
    attributes
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
pub(super) fn collect_entities(
    stat_summaries: impl IntoIterator<Item = Arc<MetricsFetcherIngestion<StatsSummary>>>,
    cluster_name: &str,
    owner_graph: &OwnerGraph,
) -> Vec<KubeEntity> {
    let mut out: Vec<KubeEntity> = Vec::new();
    let now = epoch_nanos();
    for summary in stat_summaries {
        for pod in &summary.payload.pods {
            // Attributes for the pod *and* its containers
            let p_attributes = pod_attributes(
                cluster_name,
                &summary.payload.node.node_name,
                pod,
                owner_graph,
            );

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
                        vec![],
                    ));
                }
                if let Some(cores) = c_cores {
                    p_usage_cores = Some(p_usage_cores.unwrap_or(0.0) + cores);
                    gauges.push(KubeGauge::new(
                        "container.cpu.usage",
                        "{cpu}",
                        Value::Cores(cores),
                        now,
                        vec![],
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use opentelemetry_proto::tonic::common::v1::any_value;
    use opentelemetry_proto::tonic::metrics::v1::{ResourceMetrics, metric, number_data_point};

    use crate::ingest::MetricsFetcherMetadata;
    use crate::ingest::kubelet_stats::{CPUStats, MemoryStats, Node, PodReference};
    use crate::test_support::{owner_graph, owner_ref};

    #[derive(Debug, PartialEq)]
    enum MetricValue {
        Cores(f64),
        Bytes(i64),
    }

    fn container(
        name: &str,
        working_set_bytes: Option<u64>,
        usage_nano_cores: Option<u64>,
    ) -> Container {
        Container {
            name: name.into(),
            start_time: None,
            cpu: Some(CPUStats {
                time: None,
                usage_nano_cores,
                usage_core_nano_seconds: None,
            }),
            memory: Some(MemoryStats { working_set_bytes }),
            swap: None,
        }
    }

    fn pod(name: &str, containers: Vec<Container>) -> Pod {
        Pod {
            pod_ref: PodReference {
                name: name.into(),
                namespace: "default".into(),
                uid: format!("{name}-uid"),
            },
            containers,
            volume: None,
        }
    }

    fn ingestion(pods: Vec<Pod>) -> Arc<MetricsFetcherIngestion<StatsSummary>> {
        Arc::new(MetricsFetcherIngestion {
            received_at: std::time::Instant::now(),
            metadata: MetricsFetcherMetadata::default(),
            payload: StatsSummary {
                node: Node {
                    node_name: "worker-1".into(),
                },
                pods,
            },
        })
    }

    fn attribute_value<'a>(entity: &'a ResourceMetrics, key: &str) -> Option<&'a str> {
        entity
            .resource
            .as_ref()?
            .attributes
            .iter()
            .find(|attribute| attribute.key == key)?
            .value
            .as_ref()?
            .value
            .as_ref()
            .and_then(|value| match value {
                any_value::Value::StringValue(value) => Some(value.as_str()),
                _ => None,
            })
    }

    fn entity_id(entity: &ResourceMetrics) -> String {
        let pod_name = attribute_value(entity, "k8s.pod.name")
            .expect("collected entity should identify its pod");
        match attribute_value(entity, "k8s.container.name") {
            Some(container_name) => format!("container/{pod_name}/{container_name}"),
            None => format!("pod/{pod_name}"),
        }
    }

    fn metric_values(entity: &ResourceMetrics) -> BTreeMap<String, MetricValue> {
        entity.scope_metrics[0]
            .metrics
            .iter()
            .map(|metric| {
                let metric::Data::Gauge(gauge) = metric
                    .data
                    .as_ref()
                    .expect("collected metric should have gauge data")
                else {
                    panic!("expected gauge metric")
                };
                let value = match gauge.data_points[0]
                    .value
                    .as_ref()
                    .expect("collected gauge should have a value")
                {
                    number_data_point::Value::AsDouble(value) => MetricValue::Cores(*value),
                    number_data_point::Value::AsInt(value) => MetricValue::Bytes(*value),
                };
                (metric.name.clone(), value)
            })
            .collect()
    }

    fn collect_metric_values(pods: Vec<Pod>) -> BTreeMap<String, BTreeMap<String, MetricValue>> {
        collect_entities([ingestion(pods)], "my-cluster", &owner_graph(&[]))
            .into_iter()
            .map(ResourceMetrics::from)
            .map(|entity| (entity_id(&entity), metric_values(&entity)))
            .collect()
    }

    fn attribute_map(attributes: &[Arc<Attribute>]) -> BTreeMap<&str, &str> {
        attributes
            .iter()
            .map(|attribute| (attribute.key, attribute.value.as_str()))
            .collect()
    }

    #[test]
    fn owner_chain_attributes() {
        let graph = owner_graph(&[
            ("pod-uid", owner_ref("ReplicaSet", "web-abc", "rs-uid")),
            ("rs-uid", owner_ref("Deployment", "web", "deployment-uid")),
        ]);
        let pod = Pod {
            pod_ref: PodReference {
                name: "web-abc-123".into(),
                namespace: "default".into(),
                uid: "pod-uid".into(),
            },
            containers: Vec::new(),
            volume: None,
        };

        insta::assert_json_snapshot!(attribute_map(&pod_attributes(
            "my-cluster",
            "worker-1",
            &pod,
            &graph,
        )));
    }

    #[test]
    fn unknown_owner_attributes() {
        let graph = owner_graph(&[(
            "pod-uid",
            owner_ref("ExampleController", "example", "controller-uid"),
        )]);

        insta::assert_json_snapshot!(attribute_map(&owner_attributes(&graph, "pod-uid")));
    }

    #[test]
    fn collect_entities_sums_multi_container_pod_metrics() {
        let actual = collect_metric_values(vec![pod(
            "web",
            vec![
                container("application", Some(100), Some(250_000_000)),
                container("sidecar", Some(300), Some(750_000_000)),
            ],
        )]);

        assert_eq!(
            actual,
            BTreeMap::from([
                (
                    "container/web/application".into(),
                    BTreeMap::from([
                        ("container.cpu.usage".into(), MetricValue::Cores(0.25)),
                        (
                            "container.memory.working_set".into(),
                            MetricValue::Bytes(100),
                        ),
                    ]),
                ),
                (
                    "container/web/sidecar".into(),
                    BTreeMap::from([
                        ("container.cpu.usage".into(), MetricValue::Cores(0.75)),
                        (
                            "container.memory.working_set".into(),
                            MetricValue::Bytes(300),
                        ),
                    ]),
                ),
                (
                    "pod/web".into(),
                    BTreeMap::from([
                        ("k8s.pod.cpu.usage".into(), MetricValue::Cores(1.0)),
                        ("k8s.pod.memory.working_set".into(), MetricValue::Bytes(400),),
                    ]),
                ),
            ])
        );
    }

    #[test]
    fn collect_entities_omits_missing_samples_instead_of_emitting_zeroes() {
        let actual = collect_metric_values(vec![
            pod(
                "partially-sampled",
                vec![
                    container("sampled", Some(64), None),
                    container("without-samples", None, None),
                ],
            ),
            pod(
                "without-samples",
                vec![container("without-samples", None, None)],
            ),
        ]);

        assert_eq!(
            actual,
            BTreeMap::from([
                (
                    "container/partially-sampled/sampled".into(),
                    BTreeMap::from([(
                        "container.memory.working_set".into(),
                        MetricValue::Bytes(64),
                    )]),
                ),
                (
                    "pod/partially-sampled".into(),
                    BTreeMap::from([
                        ("k8s.pod.memory.working_set".into(), MetricValue::Bytes(64),)
                    ]),
                ),
            ])
        );
    }
}
