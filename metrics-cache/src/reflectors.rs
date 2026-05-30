use std::fmt::Debug;
use std::hash::Hash;

use futures_util::StreamExt;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::ResourceExt;
use kube::runtime::watcher::Config as WatchConfig;
use kube::runtime::{WatchStreamExt, reflector, reflector::Store, watcher};
use kube::{Api, Client, Resource};
use serde::de::DeserializeOwned;
use tracing::{debug, error, trace};

pub fn start_reflector<K>(api: Api<K>, config: WatchConfig) -> Store<K>
where
    K: Resource + Clone + DeserializeOwned + Debug + Send + Sync + 'static,
    K::DynamicType: Default + Eq + Hash + Clone + Debug + Unpin,
{
    let (reader, writer) = reflector::store();
    let watch = reflector(writer, watcher(api, config))
        .default_backoff()
        .touched_objects()
        .for_each(|r| {
            match r {
                Ok(k) => {
                    trace!(
                        kind = %K::kind(&K::DynamicType::default()),
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

pub fn daemonsets(client: Client) -> Store<DaemonSet> {
    debug!("starting DaemonSet reflector");
    start_reflector(Api::all(client), WatchConfig::default())
}

pub fn deployments(client: Client) -> Store<Deployment> {
    debug!("starting Deployment reflector");
    start_reflector(Api::all(client), WatchConfig::default())
}

pub fn nodes(client: Client) -> Store<Node> {
    debug!("starting Node reflector");
    start_reflector(Api::all(client), WatchConfig::default())
}

pub fn pods(client: Client) -> Store<Pod> {
    debug!("starting Pod reflector");
    start_reflector(Api::all(client), WatchConfig::default())
}
