pub mod cluster;
pub mod namespace;
pub mod pod;

use crate::host_settings::HostSettings;
use crate::piggyback::cluster::Cluster;
use crate::piggyback::namespace::Namespace;
use crate::piggyback::pod::Pod;
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

/// Common, identifying data used for a given piggyback host type.
///
/// Mostly, this is used (via [`Self::piggyback_hostname()`]) to generate the
/// piggyback hostname for a given resource.
struct Meta<'a> {
    name: &'a str,
    namespace: Option<&'a str>, // None for cluster-scoped kinds
    kind: String,
}

impl<'a> Meta<'a> {
    fn from_resource<K>(api: &'a K) -> Option<Self>
    where
        K: kube::Resource + k8s_openapi::Resource,
    {
        let meta = api.meta();
        Some(Meta {
            name: meta.name.as_deref()?,
            namespace: meta.namespace.as_deref(),
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

    // Cluster is a special snowflake, there aren't any reflectors to iterate
    out.extend(collect(std::iter::once(()), |()| {
        Some(Cluster::new(snap, settings))
    }));
    out
}
