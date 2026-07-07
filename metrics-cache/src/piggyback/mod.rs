pub mod aggregation_host;
pub mod cluster;
pub mod daemonset;
pub mod deployment;
pub mod namespace;
pub mod node;
pub mod pod;
pub mod statefulset;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::{ClusterResourceScope, NamespaceResourceScope};

use crate::host_settings::HostSettings;
pub(crate) use crate::piggyback::aggregation_host::AggregationHost;
use crate::piggyback::cluster::Cluster;
use crate::piggyback::daemonset::DaemonSet;
use crate::piggyback::deployment::Deployment;
use crate::piggyback::namespace::Namespace;
use crate::piggyback::node::Node;
use crate::piggyback::pod::Pod;
use crate::piggyback::statefulset::StatefulSet;
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

trait Scoped {
    const NAMESPACED: bool;
}

impl Scoped for ClusterResourceScope {
    const NAMESPACED: bool = false;
}

impl Scoped for NamespaceResourceScope {
    const NAMESPACED: bool = true;
}

/// Common, identifying data used for a given piggyback host type.
///
/// Mostly, this is used (via [`Self::piggyback_hostname()`]) to generate the
/// piggyback hostname for a given resource.
#[derive(Debug)]
struct Meta<'a> {
    name: &'a str,
    namespace: Option<&'a str>, // None for cluster-scoped kinds
    kind: String,
}

impl<'a> Meta<'a> {
    fn from_resource<K>(api: &'a K) -> Option<Self>
    where
        K: k8s_openapi::Metadata<Ty = ObjectMeta>,
        K::Scope: Scoped,
    {
        let meta = api.metadata();
        let namespace = meta.namespace.as_deref();

        if K::Scope::NAMESPACED && namespace.is_none() {
            return None;
        }

        Some(Meta {
            name: meta.name.as_deref()?,
            namespace,
            kind: K::KIND.to_lowercase(),
        })
    }

    fn piggyback_hostname(&self, cluster: &str) -> String {
        match self.namespace {
            Some(namespace) => {
                format!("{}_{}_{}_{}", self.kind, cluster, namespace, self.name)
            }
            None => format!("{}_{}_{}", self.kind, cluster, self.name),
        }
    }
}

/// Represents a piggyback host for which to emit/write a section data.
pub(crate) trait PiggybackHost {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>>;
}

fn collect<A, H: PiggybackHost>(
    items: impl Iterator<Item = A>,
    make: impl Fn(A) -> Option<H>,
) -> Vec<WriteableSection> {
    items
        .filter_map(make)
        .flat_map(|host| host.emit())
        .filter_map(|r| match r {
            Ok(section) => Some(section),
            Err(e) => {
                tracing::warn!(section = %e.name, error = %e.source, "skipping section");
                None
            }
        })
        .collect()
}

pub fn emit_all(snap: &Snapshot, settings: &HostSettings) -> Vec<WriteableSection> {
    let mut out = Vec::new();
    out.extend(collect(snap.stores.pods.iter(), |p| {
        Pod::new(p, snap, settings)
    }));
    out.extend(collect(snap.stores.namespaces.iter(), |n| {
        Namespace::new(n, snap, settings)
    }));
    out.extend(collect(snap.stores.nodes.iter(), |n| {
        Node::new(n, snap, settings)
    }));
    out.extend(collect(snap.stores.deployments.iter(), |n| {
        Deployment::new(n, snap, settings)
    }));
    out.extend(collect(snap.stores.daemonsets.iter(), |n| {
        DaemonSet::new(n, snap, settings)
    }));
    out.extend(collect(snap.stores.statefulsets.iter(), |n| {
        StatefulSet::new(n, snap, settings)
    }));

    // Cluster is a special snowflake, there aren't any reflectors to iterate
    out.extend(collect(std::iter::once(()), |()| {
        Some(Cluster::new(snap, settings))
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::assert_matches;

    use crate::test_support::*;

    /// Namespace-scoped resources with no namespace are rejected by our [`Meta`].
    #[test]
    fn meta_from_resource_rejects_namespaceless_namespace_scoped_resources() {
        let mut pod = pod("pod-1", Some("node01"));
        assert_matches!(pod.metadata.namespace, None); // sanity
        assert_matches!(Meta::from_resource(&pod), None);

        pod.metadata.namespace = Some("my-ns".to_string());
        assert_matches!(Meta::from_resource(&pod), Some(_));
    }

    /// Generation of hostnames for cluster and namespace-scoped resources.
    #[test]
    fn meta_piggyback_hostname() {
        // Cluster-scope
        assert_eq!(
            Meta::from_resource(&node("node-1"))
                .unwrap()
                .piggyback_hostname("mycluster"),
            "node_mycluster_node-1",
        );

        // Namespaced
        let mut pod = pod("pod-1", Some("node01"));
        pod.metadata.namespace = Some("my-ns".to_string());
        assert_eq!(
            Meta::from_resource(&pod)
                .unwrap()
                .piggyback_hostname("mycluster"),
            "pod_mycluster_my-ns_pod-1"
        );
    }
}
