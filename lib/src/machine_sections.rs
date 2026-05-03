use crate::metadata;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sections {
    pub node_name: String,
    pub sections: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MachineSections {
    pub sections: Sections,
    pub metadata: metadata::metrics_fetcher::Metadata,
}
