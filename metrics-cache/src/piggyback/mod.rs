pub mod pod;

use crate::snapshot::Snapshot;
use crate::writeable_section::{SectionError, WriteableSection};

/// Common, identifying data used for a given piggyback host type.
///
/// Mostly, this is used (via [`self::piggyback_hostname()`]) to generate the
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
    /// Given a [`Snapshot`], generate _all_ of the required Checkmk sections
    /// for this piggyback host.
    fn emit(&self, snapshot: &Snapshot) -> Vec<Result<WriteableSection, SectionError>>;
}
