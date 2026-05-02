pub mod metrics_fetcher;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Platform {
    pub os_name: String,
    pub os_version: String,
    pub python_version: String, // Not used in Checkmk, but still a required field
    pub python_compiler: String, // Not used in Checkmk, but still a required field
}
