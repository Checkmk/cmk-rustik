use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KubeletHealth {
    Response { status_code: u16, response: String },
    ConnectionError { message: String },
}
