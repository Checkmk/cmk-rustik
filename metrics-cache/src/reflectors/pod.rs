use futures_util::stream::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::{
    WatchStreamExt,
    reflector::{self, Store, reflector},
    watcher,
};

use kube::{Api, Client, ResourceExt};
use std::future;

pub fn pod_reflector(client: Client) -> Store<Pod> {
    let api: Api<Pod> = Api::all(client);
    let (reader, writer) = reflector::store();
    let watch = reflector(writer, watcher(api, Default::default()))
        .default_backoff()
        .touched_objects()
        .for_each(|r| {
            match r {
                Ok(o) => println!("Saw {} in {}", o.name_any(), o.namespace().unwrap()),
                Err(e) => println!("Watcher error: {e}"),
            };
            future::ready(())
        });
    tokio::spawn(watch);
    reader
}
