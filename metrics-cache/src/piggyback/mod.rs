pub mod aggregation_host;
pub mod cluster;
pub mod cronjob;
pub mod daemonset;
pub mod deployment;
pub mod namespace;
pub mod node;
pub mod pod;
pub mod statefulset;

use k8s_openapi::api::core::v1;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::{ClusterResourceScope, NamespaceResourceScope};
use std::collections::HashSet;
use tracing::warn;

use crate::host_settings::{HostSettings, NamespaceFilter};
pub(crate) use crate::piggyback::aggregation_host::AggregationHost;
use crate::piggyback::cluster::Cluster;
use crate::piggyback::cronjob::CronJob;
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
    fn metadata(&self) -> Option<&ObjectMeta>;
    fn kind(&self) -> &str;
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>>;

    /// The namespace used to decide whether this host is filtered. Cluster
    /// resources return `None` and are unaffected by namespace filtering.
    fn namespace_for_filtering(&self) -> Option<&str> {
        self.metadata()?.namespace.as_deref()
    }

    /// Answers the question: Does an annotation request this resource become
    /// a piggyback host?
    ///
    /// With an annotation like:
    ///
    /// ```yaml
    /// annotations:
    ///   checkmk.com/promote-to-host: "true"
    /// ```
    ///
    /// in a resource's mewtadata, we will promote it to become a piggyback host
    /// in Checkmk (assuming we support monitoring the resource).
    ///
    /// There are also CLI flags (set via helm chart variables) that control at
    /// the kind-level which resources get promoted. We do not check that *here*
    /// but rather in [`crate::piggyback::collect()`] /
    /// [`crate::piggyback::should_emit()`]. A valid true/false annotation
    /// overrides the global setting in all cases, otherwise the kind's global
    /// flag decides.
    ///
    /// If the value is "true" or "false, we return the boolean of it, otherwise
    /// we return None and log a warning with identifying info.
    fn annotation_emit_override(&self) -> Option<bool> {
        fn parse_value(val: &str) -> Result<bool, &str> {
            match val {
                "true" => Ok(true),
                "false" => Ok(false),
                other => Err(other),
            }
        }

        let metadata = self.metadata()?;
        let value = metadata
            .annotations
            .as_ref()?
            .get("checkmk.com/promote-to-host")?;

        match parse_value(value) {
            Ok(b) => Some(b),
            Err(orig) => {
                warn!(
                    value = orig,
                    namespace = metadata.namespace,
                    name = metadata.name,
                    kind = self.kind(),
                    "unknown promote-to-host value, should be 'true' or 'false'"
                );
                None
            }
        }
    }
}

fn should_emit<H: PiggybackHost>(
    resource: &H,
    always_emit: bool,
    namespace_filter: &NamespaceFilter,
) -> bool {
    resource
        .namespace_for_filtering()
        .is_none_or(|namespace| namespace_filter.is_included(namespace))
        && resource.annotation_emit_override().unwrap_or(always_emit)
}

fn collect<A, H: PiggybackHost>(
    items: impl Iterator<Item = A>,
    always_emit: bool,
    namespace_filter: &NamespaceFilter,
    make: impl Fn(A) -> Option<H>,
) -> Vec<WriteableSection> {
    items
        .filter_map(make)
        .filter(|host| should_emit(host, always_emit, namespace_filter))
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

fn is_pod_host_candidate(pod: &v1::Pod, excluded_uids: &HashSet<&str>) -> bool {
    pod.metadata
        .uid
        .as_deref()
        .is_none_or(|uid| !excluded_uids.contains(uid))
}

pub fn emit_all(snap: &Snapshot, settings: &HostSettings) -> Vec<WriteableSection> {
    let always = &settings.always_emitted;
    let namespace_filter = &settings.namespace_filter;
    let cronjob_pod_uids = if settings.include_cronjob_pods {
        HashSet::new()
    } else {
        snap.stores
            .cronjobs
            .iter()
            .filter_map(|cronjob| cronjob.metadata.uid.as_deref())
            .flat_map(|uid| snap.owner_graph.pods_by_controller(uid))
            .filter_map(|pod| pod.metadata.uid.as_deref())
            .collect()
    };
    let mut out = Vec::new();
    out.extend(collect(
        snap.stores
            .pods
            .iter()
            .filter(|pod| is_pod_host_candidate(pod, &cronjob_pod_uids)),
        always.pods,
        namespace_filter,
        |p| Pod::new(p, snap, settings),
    ));
    out.extend(collect(
        snap.stores.namespaces.iter(),
        always.namespaces,
        namespace_filter,
        |n| Namespace::new(n, snap, settings),
    ));
    out.extend(collect(
        snap.stores.nodes.iter(),
        always.nodes,
        namespace_filter,
        |n| Node::new(n, snap, settings),
    ));
    out.extend(collect(
        snap.stores.deployments.iter(),
        always.deployments,
        namespace_filter,
        |n| Deployment::new(n, snap, settings),
    ));
    out.extend(collect(
        snap.stores.daemonsets.iter(),
        always.daemonsets,
        namespace_filter,
        |n| DaemonSet::new(n, snap, settings),
    ));
    out.extend(collect(
        snap.stores.statefulsets.iter(),
        always.statefulsets,
        namespace_filter,
        |n| StatefulSet::new(n, snap, settings),
    ));
    out.extend(collect(
        snap.stores.cronjobs.iter(),
        always.cronjobs,
        namespace_filter,
        |n| CronJob::new(n, snap, settings),
    ));

    // Cluster is a special snowflake, there aren't any reflectors to iterate
    out.extend(collect(std::iter::once(()), true, namespace_filter, |()| {
        Some(Cluster::new(snap, settings))
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use serde::Serialize;
    use std::assert_matches;
    use std::sync::Arc;

    use crate::section::Section;
    use crate::test_support::*;

    #[derive(Serialize)]
    struct FakeSection;
    impl Section for FakeSection {
        const NAME: &'static str = "fake_section_v42";
    }

    struct FakeHost(Option<ObjectMeta>);
    impl PiggybackHost for FakeHost {
        fn metadata(&self) -> Option<&ObjectMeta> {
            self.0.as_ref()
        }

        fn kind(&self) -> &str {
            "fake"
        }

        fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
            vec![WriteableSection::of("the-hostname", &FakeSection {})]
        }
    }

    fn meta_with_annotation(value: &str) -> ObjectMeta {
        ObjectMeta {
            annotations: Some(
                [("checkmk.com/promote-to-host".to_string(), value.to_string())].into(),
            ),
            ..Default::default()
        }
    }

    /// Piggyback hosts are created iff they are annotated appropriately *or*
    /// their kind's "always emit" flag is set (via CLI arg -> HostSettings).
    #[test]
    fn collect_filtering() {
        for (always, meta, emitted_count) in [
            // always-emit is false and there is no annotation -> don't emit
            (false, None, 0),
            // always-emit is true and there is no annotation -> emit
            (true, None, 1),
            // always-emit is false and annotation is "true" -> emit
            (false, Some(meta_with_annotation("true")), 1),
            // always-emit is false and annotation is "True" -> do -not- emit
            // (must be exactly "true").
            (false, Some(meta_with_annotation("True")), 0),
            // always-emit is true and annotation is "untrue" -> emit (global
            // flag rules)
            (true, Some(meta_with_annotation("untrue")), 1),
            // always-emit is true and annotation is "false" -> don't emit
            // (opt out of global-enabled)
            (true, Some(meta_with_annotation("false")), 0),
        ] {
            assert_eq!(
                collect(
                    std::iter::once(()),
                    always,
                    &NamespaceFilter::default(),
                    |()| Some(FakeHost(meta.clone()))
                )
                .len(),
                emitted_count,
                "always={always:?}, meta={meta:?}"
            );
        }
    }

    #[test]
    fn namespace_filtering_precedes_annotation_promotion() {
        let mut metadata = meta_with_annotation("true");
        metadata.namespace = Some("development".to_string());
        let filter = NamespaceFilter::new(vec![Regex::new("^production$").unwrap()], vec![]);

        assert!(
            collect(std::iter::once(()), false, &filter, |()| Some(FakeHost(
                Some(metadata.clone())
            )))
            .is_empty()
        );
    }

    #[test]
    fn cronjob_pods_are_excluded_from_pod_hosts() {
        let mut cronjob_pod = pod("cronjob-pod", Some("node01"));
        cronjob_pod.metadata.uid = Some(s("cronjob-pod-uid"));
        let mut other_pod = pod("other-pod", Some("node01"));
        other_pod.metadata.uid = Some(s("other-pod-uid"));
        let mut graph = owner_graph(&[]);
        graph
            .pods_by_controller
            .insert("cronjob-uid".into(), vec![Arc::new(cronjob_pod)]);
        graph
            .pods_by_controller
            .insert("other-uid".into(), vec![Arc::new(other_pod)]);

        let uids = graph
            .pods_by_controller("cronjob-uid")
            .iter()
            .filter_map(|pod| pod.metadata.uid.as_deref())
            .collect();

        assert_eq!(uids, HashSet::from(["cronjob-pod-uid"]));
        assert!(!is_pod_host_candidate(
            &graph.pods_by_controller("cronjob-uid")[0],
            &uids
        ));
        assert!(is_pod_host_candidate(
            &graph.pods_by_controller("other-uid")[0],
            &uids
        ));
        assert!(is_pod_host_candidate(
            &graph.pods_by_controller("cronjob-uid")[0],
            &HashSet::new()
        ));
    }

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
