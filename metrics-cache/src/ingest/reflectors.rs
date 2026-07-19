use futures_util::StreamExt;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::core::v1::{Namespace, Node, PersistentVolume, PersistentVolumeClaim, Pod};
use kube::runtime::reflector::store::WriterDropped;
use kube::runtime::watcher::Config as WatchConfig;
use kube::runtime::{WatchStreamExt, reflector, reflector::Store, watcher};
use kube::{Api, Client, Resource, ResourceExt};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::IntoIterator;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tracing::{debug, error, trace};

/// The [`Store`]s where all of the watched Kubernetes kinds that rustik
/// monitors end up. The stores here are not frozen and might update in the
/// background at any time.
#[derive(Clone)]
pub struct Stores {
    pub pods: Store<Pod>,
    pub nodes: Store<Node>,
    pub deployments: Store<Deployment>,
    pub daemonsets: Store<DaemonSet>,
    pub namespaces: Store<Namespace>,
    pub replicasets: Store<ReplicaSet>,
    pub persistent_volumes: Store<PersistentVolume>,
    pub persistent_volume_claims: Store<PersistentVolumeClaim>,
    pub statefulsets: Store<StatefulSet>,
    pub(crate) healths: ReflectorHealthHandles,
}

#[derive(Debug)]
pub struct FrozenStores {
    pub pods: Vec<Arc<Pod>>,
    pub nodes: Vec<Arc<Node>>,
    pub deployments: Vec<Arc<Deployment>>,
    pub daemonsets: Vec<Arc<DaemonSet>>,
    pub namespaces: Vec<Arc<Namespace>>,
    pub replicasets: Vec<Arc<ReplicaSet>>,
    pub persistent_volumes: Vec<Arc<PersistentVolume>>,
    pub persistent_volume_claims: Vec<Arc<PersistentVolumeClaim>>,
    pub statefulsets: Vec<Arc<StatefulSet>>,
}

impl Stores {
    pub(crate) fn spawn(client: Client, tasks: &mut JoinSet<()>) -> Self {
        let healths = ReflectorHealthHandles::default();

        Self {
            pods: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.pods.clone(),
            ),
            nodes: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.nodes.clone(),
            ),
            deployments: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.deployments.clone(),
            ),
            daemonsets: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.daemonsets.clone(),
            ),
            namespaces: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.namespaces.clone(),
            ),
            replicasets: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.replicasets.clone(),
            ),
            persistent_volumes: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.persistent_volumes.clone(),
            ),
            persistent_volume_claims: start_reflector(
                Api::all(client.clone()),
                WatchConfig::default(),
                tasks,
                healths.persistent_volume_claims.clone(),
            ),
            statefulsets: start_reflector(
                Api::all(client),
                WatchConfig::default(),
                tasks,
                healths.statefulsets.clone(),
            ),
            healths,
        }
    }

    pub(crate) fn freeze(&self) -> FrozenStores {
        FrozenStores {
            pods: self.pods.state(),
            nodes: self.nodes.state(),
            deployments: self.deployments.state(),
            daemonsets: self.daemonsets.state(),
            namespaces: self.namespaces.state(),
            replicasets: self.replicasets.state(),
            persistent_volumes: self.persistent_volumes.state(),
            persistent_volume_claims: self.persistent_volume_claims.state(),
            statefulsets: self.statefulsets.state(),
        }
    }

    pub(crate) fn freeze_healths(&self) -> FrozenReflectorHealths {
        self.healths.freeze()
    }

    pub(crate) async fn wait_until_all_ready(&self) -> Result<(), WriterDropped> {
        tokio::try_join!(
            self.pods.wait_until_ready(),
            self.nodes.wait_until_ready(),
            self.deployments.wait_until_ready(),
            self.daemonsets.wait_until_ready(),
            self.namespaces.wait_until_ready(),
            self.replicasets.wait_until_ready(),
            self.persistent_volumes.wait_until_ready(),
            self.persistent_volume_claims.wait_until_ready(),
            self.statefulsets.wait_until_ready(),
        )?;
        Ok(())
    }
}

/// The inner-state of a reflector. This gets updated by the reflector's
/// `inspect` callback as certain events happen (namely `Init`, `InitDone`, and
/// errors).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ReflectorHealth {
    /// Whether this reflector has completed at least one full initialization
    /// and received its first `InitDone` event. It is never reset, so as long
    /// as the reflector has contained at least one complete list of the watched
    /// resource, it will be `true`.
    pub(crate) has_been_initialized: bool,
    /// When the current full list or relist began.
    ///
    /// This is set on `Init` and cleared on `InitDone`. It is also set during
    /// the initial listing on startup, before `has_been_initialized` is true.
    pub(crate) relist_started_at: Option<Instant>,
    /// When the last full list or relist finished.
    ///
    /// Set on `InitDone`.
    pub(crate) relist_completed_at: Option<Instant>,
    /// How long the last full relist took. Notably, this includes the time
    /// from the list request, including network transfer, response
    /// deserialization, `InitApply` processing for every object, reflector
    /// store population/swap (atomic) and the final `InitDone`.
    ///
    /// This does not, on its own, measure Kubernetes API server latency, but it
    /// could be an indication that something is wrong. The initial list counts
    /// as the first duration.
    pub(crate) relist_duration: Option<Duration>,
    /// When the most recent watcher error was observed.
    ///
    /// This includes errors yielded by the kube-runtime watcher while listing,
    /// starting a watch, or consuming a watch stream.
    pub(crate) last_error_at: Option<Instant>,
    /// How many errors, in total, has the reflector seen in its lifetime?
    pub(crate) errors_total: u64,
}

#[derive(Clone, Debug, Default)]
struct ReflectorHealthHandle(Arc<Mutex<ReflectorHealth>>);

impl ReflectorHealthHandle {
    fn observe<K>(&self, result: &Result<watcher::Event<K>, watcher::Error>) {
        match result {
            Ok(watcher::Event::Init) => {
                let now = Instant::now();
                let mut health = self.0.lock();
                health.relist_started_at = Some(now);
            }
            Ok(watcher::Event::InitDone) => {
                let now = Instant::now();
                let mut health = self.0.lock();
                if let Some(started_at) = health.relist_started_at.take() {
                    health.relist_duration = Some(now.saturating_duration_since(started_at));
                }
                health.has_been_initialized = true;
                health.relist_completed_at = Some(now);
            }
            Ok(_) => {}
            Err(_) => {
                let now = Instant::now();
                let mut health = self.0.lock();
                health.errors_total = health.errors_total.saturating_add(1);
                health.last_error_at = Some(now);
            }
        }
    }

    fn freeze(&self) -> ReflectorHealth {
        *self.0.lock()
    }
}

/// The collection of [`ReflectorHealthHandle`]s which are constantly being
/// changed by the reflector as events happen. Thus, we _can not_ rely on this
/// for a snapshot; at snapshot-time, it is only used to generate a
/// [`FrozenReflectorHealths`] via [`Self::freeze()`].
#[derive(Clone, Debug, Default)]
pub(crate) struct ReflectorHealthHandles {
    pods: ReflectorHealthHandle,
    nodes: ReflectorHealthHandle,
    deployments: ReflectorHealthHandle,
    daemonsets: ReflectorHealthHandle,
    namespaces: ReflectorHealthHandle,
    replicasets: ReflectorHealthHandle,
    persistent_volumes: ReflectorHealthHandle,
    persistent_volume_claims: ReflectorHealthHandle,
    statefulsets: ReflectorHealthHandle,
}

impl ReflectorHealthHandles {
    fn freeze(&self) -> FrozenReflectorHealths {
        FrozenReflectorHealths {
            pods: self.pods.freeze(),
            nodes: self.nodes.freeze(),
            deployments: self.deployments.freeze(),
            daemonsets: self.daemonsets.freeze(),
            namespaces: self.namespaces.freeze(),
            replicasets: self.replicasets.freeze(),
            persistent_volumes: self.persistent_volumes.freeze(),
            persistent_volume_claims: self.persistent_volume_claims.freeze(),
            statefulsets: self.statefulsets.freeze(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FrozenReflectorHealths {
    pub(crate) pods: ReflectorHealth,
    pub(crate) nodes: ReflectorHealth,
    pub(crate) deployments: ReflectorHealth,
    pub(crate) daemonsets: ReflectorHealth,
    pub(crate) namespaces: ReflectorHealth,
    pub(crate) replicasets: ReflectorHealth,
    pub(crate) persistent_volumes: ReflectorHealth,
    pub(crate) persistent_volume_claims: ReflectorHealth,
    pub(crate) statefulsets: ReflectorHealth,
}

const REFLECTOR_COUNT: usize = 9;

// Pardon the "cute" IntoIter here; it avoids having to edit
// crate::snapshot::self_health every time a new reflector is added here.
impl IntoIterator for FrozenReflectorHealths {
    type Item = (&'static str, ReflectorHealth);
    type IntoIter = std::array::IntoIter<Self::Item, REFLECTOR_COUNT>;

    fn into_iter(self) -> Self::IntoIter {
        [
            ("pods", self.pods),
            ("nodes", self.nodes),
            ("deployments", self.deployments),
            ("daemonsets", self.daemonsets),
            ("namespaces", self.namespaces),
            ("replicasets", self.replicasets),
            ("persistent_volumes", self.persistent_volumes),
            ("persistent_volume_claims", self.persistent_volume_claims),
            ("statefulsets", self.statefulsets),
        ]
        .into_iter()
    }
}

fn start_reflector<K>(
    api: Api<K>,
    config: WatchConfig,
    tasks: &mut JoinSet<()>,
    health: ReflectorHealthHandle,
) -> Store<K>
where
    K: Resource + Clone + DeserializeOwned + Debug + Send + Sync + k8s_openapi::Resource + 'static,
    K::DynamicType: Default + Eq + Hash + Clone + Debug + Unpin,
{
    debug!(kind = K::KIND, "starting reflector");
    let (reader, writer) = reflector::store();
    let watch = reflector(writer, watcher(api, config))
        .modify(|k| {
            k.managed_fields_mut().clear();
        })
        .inspect(move |result| health.observe(result))
        .default_backoff()
        .touched_objects()
        .for_each(|r| {
            match r {
                Ok(k) => {
                    trace!(
                        kind = K::KIND,
                        name = %k.name_any(),
                        namespace = ?k.namespace(),
                        "object touched"
                    )
                }
                Err(e) => error!(error = %e, "watcher error"),
            }
            std::future::ready(())
        });
    tasks.spawn(watch);
    reader
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflector_health_handle_default() {
        let outer = ReflectorHealthHandle::default();
        let handle = outer.0.lock();
        assert!(!handle.has_been_initialized);
        assert!(handle.relist_started_at.is_none());
        assert!(handle.relist_completed_at.is_none());
        assert!(handle.relist_duration.is_none());
        assert!(handle.last_error_at.is_none());
        assert_eq!(handle.errors_total, 0);
    }

    /// On Init, has_been_initialized is still not set, start timestamp is set
    #[test]
    fn reflector_health_handle_init() {
        let outer = ReflectorHealthHandle::default();
        outer.observe(&Ok(watcher::Event::<()>::Init));
        let handle = outer.0.lock();
        assert!(!handle.has_been_initialized);
        assert!(handle.relist_started_at.is_some());
        assert!(handle.relist_completed_at.is_none());
        assert!(handle.relist_duration.is_none());
        assert!(handle.last_error_at.is_none());
        assert_eq!(handle.errors_total, 0);
    }

    /// On InitDone, timestamps get updated and has_been_initialized set
    #[test]
    fn reflector_health_handle_init_done() {
        let outer = ReflectorHealthHandle::default();
        outer.observe(&Ok(watcher::Event::<()>::Init));
        outer.observe(&Ok(watcher::Event::<()>::InitDone));
        let handle = outer.0.lock();
        assert!(handle.has_been_initialized);
        assert!(handle.relist_started_at.is_none());
        assert!(handle.relist_completed_at.is_some());
        assert!(handle.relist_duration.is_some());
        assert!(handle.last_error_at.is_none());
        assert_eq!(handle.errors_total, 0);
    }

    /// Subsequent relist keeps has_been_initialized set and updates timestamps.
    #[test]
    fn reflector_health_handle_multiple_relists() {
        let outer = ReflectorHealthHandle::default();

        outer.observe(&Ok(watcher::Event::<()>::Init));
        outer.observe(&Ok(watcher::Event::<()>::InitDone));
        let first = outer.freeze();
        assert!(first.has_been_initialized);

        let old_completed_at = Instant::now() - Duration::from_secs(60);
        let old_duration = Duration::MAX;
        outer.0.lock().relist_completed_at = Some(old_completed_at);
        outer.0.lock().relist_duration = Some(old_duration);

        outer.observe(&Ok(watcher::Event::<()>::Init));
        outer.observe(&Ok(watcher::Event::<()>::InitDone));
        let second = outer.freeze();
        assert!(second.has_been_initialized); // stays set
        assert!(second.relist_completed_at.unwrap() > old_completed_at);
        assert_ne!(second.relist_duration.unwrap(), old_duration);
    }

    /// Errors get counted and last-error timestamp increased
    #[test]
    fn reflector_health_handle_err() {
        let outer = ReflectorHealthHandle::default();

        outer.observe::<()>(&Err(watcher::Error::NoResourceVersion));
        let first = outer.freeze();
        assert!(first.last_error_at.is_some());
        assert_eq!(first.errors_total, 1);

        let old_error_at = Instant::now() - Duration::from_secs(60);
        outer.0.lock().last_error_at = Some(old_error_at);

        outer.observe::<()>(&Err(watcher::Error::NoResourceVersion));
        let second = outer.freeze();
        assert!(second.last_error_at.unwrap() > old_error_at);
        assert_eq!(second.errors_total, 2);
    }

    /// Normal, non-interesting events do not change anything.
    #[test]
    fn reflector_health_handle_ordinary_event_does_nothing() {
        let outer = ReflectorHealthHandle::default();
        let before = outer.freeze();
        outer.observe(&Ok(watcher::Event::Apply(())));
        outer.observe(&Ok(watcher::Event::Delete(())));
        outer.observe(&Ok(watcher::Event::InitApply(())));
        let after = outer.freeze();
        assert_eq!(before, after);
    }

    /// The ReflectorHealth returned by ReflectorHealthHandle::freeze() does not
    /// mutate even if the handle's inner copy does.
    #[test]
    fn reflector_health_handle_frozen_means_frozen() {
        let outer = ReflectorHealthHandle::default();
        let before = outer.freeze();
        outer.observe(&Ok(watcher::Event::<()>::Init));
        outer.observe(&Ok(watcher::Event::<()>::InitDone));
        let handle = outer.0.lock();
        assert!(handle.has_been_initialized); // sanity
        assert!(!before.has_been_initialized);
    }

    /// Handle clones share state
    #[test]
    fn reflector_health_handle_shared_state() {
        let outer = ReflectorHealthHandle::default();
        let cloned = outer.clone();
        outer.observe(&Ok(watcher::Event::<()>::Init));
        outer.observe(&Ok(watcher::Event::<()>::InitDone));
        let cloned_frozen = cloned.freeze();
        assert!(cloned_frozen.has_been_initialized);
    }
}
