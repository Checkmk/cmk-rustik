use k8s_openapi::api::core::v1;
use serde::Serialize;
use std::sync::Arc;

use crate::snapshot::OwnerGraph;

#[derive(Debug, Serialize)]
pub struct Controller {
    #[serde(rename = "type")]
    type_: String,
    name: String,
}

impl Controller {
    pub fn new(type_: String, name: String) -> Self {
        Self { type_, name }
    }
}

pub struct Pod {
    api: Arc<v1::Pod>,
    control_chain: Vec<Controller>,
}

impl Pod {
    pub fn new(api: Arc<v1::Pod>, graph: &OwnerGraph) -> Self {
        let control_chain = match &api.metadata.uid {
            Some(uid) => graph
                .walk_up(&uid)
                .iter()
                .map(|o| Controller::new(o.kind.clone(), o.name.clone()))
                .collect(),
            None => Vec::new(),
        };
        Pod { api, control_chain }
    }
}
