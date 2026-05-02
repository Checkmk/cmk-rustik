use crate::metadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct FetchResult {
    pub node_name: String,
    pub sections: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MachineSections {
    pub sections: FetchResult,
    pub metadata: metadata::MetricsFetcher,
}
