use crate::metadata;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FetchResult {
    pub node_name: String,
    pub sections: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MachineSections {
    pub sections: FetchResult,
    pub metadata: metadata::metrics_fetcher::Fetcher,
}
