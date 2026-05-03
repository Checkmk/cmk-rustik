use crate::metadata;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Metric {
    pub container_name: String,
    pub namespace: String,
    pub pod_uid: String,
    pub pod_name: String,
    pub metric_name: String,
    pub metric_value_string: String,
    pub timestamp: f64, // No sense converting to something else, we don't process it at all.
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContainerMetrics {
    pub container_metrics: Vec<Metric>,
    pub metadata: metadata::metrics_fetcher::Metadata,
}
