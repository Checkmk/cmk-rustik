#![cfg(test)]

use k8s_openapi::api::apps::v1::{
    DaemonSet, DaemonSetSpec, Deployment, DeploymentSpec, ReplicaSet,
};
use k8s_openapi::api::batch::v1::{CronJob, CronJobSpec, Job};
use k8s_openapi::api::core::v1::{
    Container, ContainerStatus, Namespace, Node, NodeAddress, NodeStatus, NodeSystemInfo,
    PersistentVolumeClaim, PersistentVolumeClaimSpec, PersistentVolumeClaimStatus, Pod, PodSpec,
    VolumeResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    LabelSelector, LabelSelectorRequirement, ObjectMeta, OwnerReference, Time,
};
use k8s_openapi::jiff::Timestamp;
use std::collections::{BTreeMap, HashMap};

use crate::host_settings::{AlwaysEmitted, AnnotationKeyPattern, HostSettings, NamespaceFilter};
use crate::snapshot::owner_graph::OwnerGraph;

#[inline(always)]
pub fn s(str: &str) -> String {
    str.to_string()
}

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
    pod.metadata.uid = Some(s("aaca3e54-7772-4af7-881f-c912008bc541"));
    pod.metadata.namespace = Some(s("the-namespace-of-all-namespaces"));
    pod.metadata.labels = Some([(s("app"), s("nginx"))].into());
    let timestamp: Timestamp = "2024-06-19 15:22:45-04"
        .parse()
        .expect("Failed to parse timestamp");
    pod.metadata.creation_timestamp = Some(Time(timestamp));

    let status = pod.status.get_or_insert_with(Default::default);
    status.qos_class = Some(s("Burstable"));
    let start_time: Timestamp = "2024-06-19 15:23:45-04"
        .parse()
        .expect("Failed to parse timestamp");
    status.start_time = Some(Time(start_time));

    pod
}

/// A Pod owned by a particular [`OwnerReference`].
pub fn pod_owned_by(name: &str, uid: &str, owner: OwnerReference) -> Pod {
    let mut pod = pod(name, Some("node"));
    pod.metadata.uid = Some(uid.to_string());
    pod.metadata.owner_references = Some(vec![owner]);
    pod
}

/// A Pod whose status reports the given `(name, image)` containers.
pub fn pod_with_container_statuses(name: &str, containers: &[(&str, &str)]) -> Pod {
    let mut pod = pod(name, Some("node01"));
    let status = pod.status.get_or_insert_with(Default::default);
    status.container_statuses = Some(
        containers
            .iter()
            .map(|(name, image)| ContainerStatus {
                name: name.to_string(),
                image: image.to_string(),
                ..Default::default()
            })
            .collect(),
    );
    pod
}

/// A Deployment, with a selector using both match forms.
pub fn deployment(name: &str) -> Deployment {
    let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
    Deployment {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(s("the-namespace-of-all-namespaces")),
            creation_timestamp: Some(Time(timestamp)),
            labels: Some(BTreeMap::from([(s("app"), name.to_string())])),
            annotations: Some(BTreeMap::from([
                (s("example.com/cool-animal"), s("monkeys")),
                (s("checkmk.com/promote-to-host"), s("true")),
            ])),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            selector: LabelSelector {
                match_labels: Some(BTreeMap::from([(s("app"), name.to_string())])),
                match_expressions: Some(vec![LabelSelectorRequirement {
                    key: s("tier"),
                    operator: s("In"),
                    values: Some(vec![s("backend"), s("frontend")]),
                }]),
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A DaemonSet, with a selector using both match forms.
pub fn daemonset(name: &str) -> DaemonSet {
    let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
    DaemonSet {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(s("the-namespace-of-all-namespaces")),
            creation_timestamp: Some(Time(timestamp)),
            labels: Some(BTreeMap::from([(s("app"), name.to_string())])),
            annotations: Some(BTreeMap::from([
                (s("example.com/cool-animal"), s("monkeys")),
                (s("checkmk.com/promote-to-host"), s("true")),
            ])),
            ..Default::default()
        },
        spec: Some(DaemonSetSpec {
            selector: LabelSelector {
                match_labels: Some(BTreeMap::from([(s("app"), name.to_string())])),
                match_expressions: Some(vec![LabelSelectorRequirement {
                    key: s("tier"),
                    operator: s("In"),
                    values: Some(vec![s("backend"), s("frontend")]),
                }]),
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A ReplicaSet owned by a particular [`OwnerReference`].
pub fn replicaset_owned_by(name: &str, uid: &str, owner: OwnerReference) -> ReplicaSet {
    ReplicaSet {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            uid: Some(uid.to_string()),
            owner_references: Some(vec![owner]),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A Job owned by a particular [`OwnerReference`].
pub fn job_owned_by(name: &str, uid: &str, owner: OwnerReference) -> Job {
    Job {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            uid: Some(uid.to_string()),
            owner_references: Some(vec![owner]),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A container, used in a Pod spec
pub fn container(name: &str) -> Container {
    Container {
        name: name.to_string(),
        image: Some(s("debian:latest")),
        command: Some(vec![s("/bin/sleep")]),
        args: Some(vec![s("1h")]),
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

pub fn node_with_roles(name: &str, roles: &[&str]) -> Node {
    let mut node = node(name);
    node.metadata.labels = Some(
        roles
            .iter()
            .map(|role| (format!("node-role.kubernetes.io/{role}"), String::new()))
            .collect(),
    );
    node
}

pub fn node_prefilled(name: &str) -> Node {
    let timestamp: Timestamp = "2026-08-07 15:22:45-04".parse().unwrap();
    let mut node = node(name);
    node.metadata.creation_timestamp = Some(Time(timestamp));
    node.metadata.labels = Some(BTreeMap::from([
        (s("kubernetes.io/hostname"), name.to_string()),
        (s("kubernetes.io/arch"), s("amd64")),
    ]));
    node.metadata.annotations = Some(BTreeMap::from([
        (s("example.com/cool-animal"), s("monkeys")),
        (s("checkmk.com/promote-to-host"), s("true")),
    ]));
    node.status = Some(NodeStatus {
        addresses: Some(vec![
            NodeAddress {
                address: s("10.0.0.5"),
                type_: s("InternalIP"),
            },
            NodeAddress {
                address: name.to_string(),
                type_: s("Hostname"),
            },
        ]),
        node_info: Some(NodeSystemInfo {
            architecture: s("amd64"),
            container_runtime_version: s("containerd://1.7.24"),
            kernel_version: s("6.8.0-51-generic"),
            kubelet_version: s("v1.34.0"),
            operating_system: s("linux"),
            os_image: s("Ubuntu 22.04.5 LTS"),
            ..Default::default()
        }),
        ..Default::default()
    });
    node
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
        jobs_by_controller: HashMap::new(),
    }
}

pub fn namespace(name: &str) -> Namespace {
    let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
    let annotations = BTreeMap::from([
        (s("example.com/cool-animal"), s("monkeys")),
        (s("checkmk.com/promote-to-host"), s("true")),
    ]);
    let labels = BTreeMap::from([(s("kubernetes.io/metadata.name"), name.to_string())]);
    Namespace {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            creation_timestamp: Some(Time(timestamp)),
            annotations: Some(annotations),
            labels: Some(labels),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn cron_job(name: &str) -> CronJob {
    let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
    let annotations = BTreeMap::from([
        (s("example.com/cool-animal"), s("monkeys")),
        (s("checkmk.com/promote-to-host"), s("true")),
    ]);
    CronJob {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(s("the-actual-coolest-namespace-of-all-time")),
            creation_timestamp: Some(Time(timestamp)),
            annotations: Some(annotations),
            ..Default::default()
        },
        spec: CronJobSpec {
            schedule: s("30 0,8,16 * * *"),
            concurrency_policy: Some(s("Allow")),
            successful_jobs_history_limit: Some(5),
            failed_jobs_history_limit: Some(10),
            suspend: Some(false),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn host_settings() -> HostSettings {
    HostSettings {
        cluster_name: s("the-cluster"),
        cluster_host_name: s("cluster.host.tld"),
        annotation_key_pattern: AnnotationKeyPattern::ImportAll,
        excluded_node_role_patterns: Vec::new(),
        namespace_filter: NamespaceFilter::default(),
        always_emitted: AlwaysEmitted::default(),
        include_cronjob_pods: false,
        emit_pvc_sections: false,
        cluster_version: s("v1.36.1"),
    }
}

pub fn pvc(name: &str) -> PersistentVolumeClaim {
    let timestamp: Timestamp = "2024-06-19 15:22:45-04".parse().unwrap();
    PersistentVolumeClaim {
        metadata: ObjectMeta {
            creation_timestamp: Some(Time(timestamp)),
            name: Some(name.to_string()),
            namespace: Some(s("really-cool-namespace")),
            ..Default::default()
        },
        spec: Some(PersistentVolumeClaimSpec {
            access_modes: Some(vec![s("ReadWriteOnce")]),
            resources: Some(VolumeResourceRequirements {
                requests: Some(BTreeMap::from([(s("storage"), Quantity(s("1Gi")))])),
                ..Default::default()
            }),
            storage_class_name: Some(s("manual")),
            volume_attributes_class_name: Some(s("silver")),
            volume_mode: Some(s("Filesystem")),
            volume_name: Some(s("test-local-pv")),
            ..Default::default()
        }),
        status: Some(PersistentVolumeClaimStatus {
            access_modes: Some(vec![s("ReadWriteOnce")]),
            capacity: Some(BTreeMap::from([(s("storage"), Quantity(s("1Gi")))])),
            current_volume_attributes_class_name: Some(s("silver")),
            phase: Some(s("Bound")),
            ..Default::default()
        }),
    }
}
