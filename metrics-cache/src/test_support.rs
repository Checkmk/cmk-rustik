#![cfg(test)]

use k8s_openapi::api::core::v1::{Node, Pod, PodSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

pub fn pod(name: &str, node: Option<&str>) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            node_name: node.map(String::from),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub fn node(name: &str) -> Node {
    Node {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        ..Default::default()
    }
}
