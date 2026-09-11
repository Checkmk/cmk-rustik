//! The OTLP wire mapping: our domain types and their translation into the
//! protobuf structures.

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value};
use opentelemetry_proto::tonic::metrics::v1::{
    Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, metric, number_data_point,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use std::sync::Arc;

pub(super) enum Value {
    /// OTel: `{cpu}`
    Cores(f64),
    /// OTel: `By`
    Bytes(i64),
}

impl From<Value> for number_data_point::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Cores(v) => Self::AsDouble(v),
            Value::Bytes(v) => Self::AsInt(v),
        }
    }
}

pub(super) struct Attribute {
    pub(super) key: &'static str,
    pub(super) value: String,
}

impl Attribute {
    pub(super) fn new(key: &'static str, value: String) -> Self {
        Self { key, value }
    }
}

impl From<&Attribute> for KeyValue {
    fn from(attribute: &Attribute) -> Self {
        KeyValue {
            key: attribute.key.into(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(attribute.value.clone())),
            }),
            ..Default::default()
        }
    }
}

pub(super) struct KubeGauge {
    name: &'static str,
    unit: &'static str,
    value: Value,
    /// Sample time. One shared instant per collection cycle, so all metrics
    /// of an export carry the same timestamp (as kubeletstats does).
    time_unix_nano: u64,
    /// Datapoint-level attributes.
    ///
    /// The kubeletstats receiver keeps entity identity purely at the resource
    /// level and leaves these empty for the metrics we mirror. We additionally
    /// stamp identity here (e.g. `k8s.container.name`) because Checkmk's OTel
    /// check derives per-graph metric names from *datapoint* attributes;
    /// without this, all containers of a pod collapse into one metric. This is
    /// additive: resource-level consumers are unaffected.
    attributes: Vec<Arc<Attribute>>,
}

impl KubeGauge {
    pub(super) fn new(
        name: &'static str,
        unit: &'static str,
        value: Value,
        time_unix_nano: u64,
        attributes: Vec<Arc<Attribute>>,
    ) -> Self {
        Self {
            name,
            unit,
            value,
            time_unix_nano,
            attributes,
        }
    }
}

impl From<KubeGauge> for Metric {
    fn from(gauge: KubeGauge) -> Self {
        Metric {
            name: gauge.name.to_string(),
            unit: gauge.unit.to_string(),
            data: Some(metric::Data::Gauge(Gauge {
                data_points: vec![NumberDataPoint {
                    time_unix_nano: gauge.time_unix_nano,
                    attributes: gauge.attributes.iter().map(|a| a.as_ref().into()).collect(),
                    value: Some(gauge.value.into()),
                    ..Default::default()
                }],
            })),
            ..Default::default()
        }
    }
}

/// A specific "thing" being reported on, e.g. a Pod or container
///
/// This is the main shape we care about for encoding to OTel format.
/// It encapsulates just enough to send valid OTel metrics.
pub(super) struct KubeEntity {
    attributes: Vec<Arc<Attribute>>,
    gauges: Vec<KubeGauge>,
}

impl KubeEntity {
    pub(super) fn new(attributes: Vec<Arc<Attribute>>, gauges: Vec<KubeGauge>) -> Self {
        Self { attributes, gauges }
    }
}

impl From<KubeEntity> for ResourceMetrics {
    fn from(entity: KubeEntity) -> Self {
        ResourceMetrics {
            resource: Some(Resource {
                attributes: entity
                    .attributes
                    .iter()
                    .map(|a| a.as_ref().into())
                    .collect(),
                ..Default::default()
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "cmk-rustik".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    ..Default::default()
                }),
                metrics: entity.gauges.into_iter().map(Metric::from).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}

impl FromIterator<KubeEntity> for ExportMetricsServiceRequest {
    fn from_iter<I: IntoIterator<Item = KubeEntity>>(iter: I) -> Self {
        ExportMetricsServiceRequest {
            resource_metrics: iter.into_iter().map(ResourceMetrics::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kube_entities_map_to_otlp_metrics() {
        let namespace = Arc::new(Attribute::new("k8s.namespace.name", "default".into()));
        let pod = Arc::new(Attribute::new("k8s.pod.name", "web-123".into()));
        let node = Arc::new(Attribute::new("k8s.node.name", "worker-1".into()));
        let cluster = Arc::new(Attribute::new("k8s.cluster.name", "production".into()));
        let container = Arc::new(Attribute::new("k8s.container.name", "application".into()));

        let entities = vec![
            KubeEntity::new(
                vec![
                    namespace.clone(),
                    pod.clone(),
                    node.clone(),
                    cluster.clone(),
                    container.clone(),
                ],
                vec![
                    KubeGauge::new(
                        "container.memory.working_set",
                        "By",
                        Value::Bytes(512),
                        123_456_789,
                        vec![container.clone()],
                    ),
                    KubeGauge::new(
                        "container.cpu.usage",
                        "{cpu}",
                        Value::Cores(0.5),
                        123_456_789,
                        vec![container],
                    ),
                ],
            ),
            KubeEntity::new(
                vec![namespace, pod, node, cluster],
                vec![
                    KubeGauge::new(
                        "k8s.pod.memory.working_set",
                        "By",
                        Value::Bytes(1_024),
                        123_456_789,
                        vec![],
                    ),
                    KubeGauge::new(
                        "k8s.pod.cpu.usage",
                        "{cpu}",
                        Value::Cores(0.75),
                        123_456_789,
                        vec![],
                    ),
                ],
            ),
        ];

        let request: ExportMetricsServiceRequest = entities.into_iter().collect();

        insta::assert_debug_snapshot!(request);
    }
}
