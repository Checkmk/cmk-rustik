use std::fmt::Debug;
use std::hash::Hash;

use futures_util::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::watcher::Config as WatchConfig;
use kube::runtime::{WatchStreamExt, reflector, reflector::Store, watcher};
use kube::{Api, Client, Resource};
use serde::de::DeserializeOwned;
use tracing::{debug, error};

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
            if let Err(e) = r {
                error!("watcher error: {e}");
            }
            std::future::ready(())
        });
    tokio::spawn(watch);
    reader
}

pub fn pods(client: Client) -> Store<Pod> {
    debug!("starting Pod reflector");
    start_reflector(Api::all(client), WatchConfig::default())
}
