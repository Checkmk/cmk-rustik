use futures_util::StreamExt;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet};
use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
use kube::runtime::watcher::Config as WatchConfig;
use kube::runtime::{WatchStreamExt, reflector, reflector::Store, watcher};
use kube::{Api, Client, Resource, ResourceExt};
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use tracing::{debug, error, trace};

#[derive(Clone)]
pub struct Stores {
    pub pods: Store<Pod>,
    pub nodes: Store<Node>,
    pub deployments: Store<Deployment>,
    pub daemonsets: Store<DaemonSet>,
    pub namespaces: Store<Namespace>,
    pub replicasets: Store<ReplicaSet>,
}

#[derive(Debug)]
pub struct FrozenStores {
    pub pods: Vec<Arc<Pod>>,
    pub nodes: Vec<Arc<Node>>,
    pub deployments: Vec<Arc<Deployment>>,
    pub daemonsets: Vec<Arc<DaemonSet>>,
    pub namespaces: Vec<Arc<Namespace>>,
    pub replicasets: Vec<Arc<ReplicaSet>>,
}

impl Stores {
    pub fn spawn(client: Client) -> Self {
        Self {
            pods: start_reflector(Api::all(client.clone()), WatchConfig::default()),
            nodes: start_reflector(Api::all(client.clone()), WatchConfig::default()),
            deployments: start_reflector(Api::all(client.clone()), WatchConfig::default()),
            daemonsets: start_reflector(Api::all(client.clone()), WatchConfig::default()),
            namespaces: start_reflector(Api::all(client.clone()), WatchConfig::default()),
            replicasets: start_reflector(Api::all(client), WatchConfig::default()),
        }
    }

    pub fn freeze(&self) -> FrozenStores {
        FrozenStores {
            pods: self.pods.state(),
            nodes: self.nodes.state(),
            deployments: self.deployments.state(),
            daemonsets: self.daemonsets.state(),
            namespaces: self.namespaces.state(),
            replicasets: self.replicasets.state(),
        }
    }
}

pub fn start_reflector<K>(api: Api<K>, config: WatchConfig) -> Store<K>
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
    tokio::spawn(watch);
    reader
}
