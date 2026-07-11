#![cfg(test)]

use k8s_openapi::api::core::v1::{Node, Pod, PodSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference, Time};
use k8s_openapi::jiff::Timestamp;

use std::collections::HashMap;

use crate::snapshot::owner_graph::OwnerGraph;

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

/// A pod, but with a lot of data filled in. Data is static, not randomly
/// generated. Tests are expected to mutate the returned Pod to their specific
/// needs. But for a simple "test needs any random pod", it should work as-is.
/// Fields might be added in the future.
pub fn pod_prefilled(name: &str) -> Pod {
    let mut pod = pod(name, Some("node01"));
    pod.metadata.uid = Some("aaca3e54-7772-4af7-881f-c912008bc541".to_string());
    pod.metadata.namespace = Some("the-namespace-of-all-namespaces".to_string());
    pod.metadata.labels = Some([("app".to_string(), "nginx".to_string())].into());
    let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
    pod.metadata.creation_timestamp = Some(Time(timestamp));
    pod.status.get_or_insert_with(Default::default).qos_class = Some("Burstable".to_string());
    pod
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

pub fn owner_ref(kind: &str, name: &str, uid: &str) -> OwnerReference {
    OwnerReference {
        kind: kind.into(),
        name: name.into(),
        uid: uid.into(),
        controller: Some(true),
        ..Default::default()
    }
}

pub fn owner_graph(edges: &[(&str, OwnerReference)]) -> OwnerGraph {
    OwnerGraph {
        owner_ref_by_uid: edges
            .iter()
            .map(|(uid, owner)| ((*uid).into(), owner.clone()))
            .collect(),
        pods_by_controller: HashMap::new(),
    }
}
