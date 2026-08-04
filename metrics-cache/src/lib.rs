#![allow(rustdoc::private_intra_doc_links)]
//! <img src="https://checkmk.com/application/files/5117/5913/5836/checkmk-logo-green-on-white.png">
//!
//! <div class="warning">
//!
//! This documentation is meant for **developers** of _cmk-rustik_ and for
//! community members wishing to contribute to the project. It is expressly
//! **not** meant as an end-user guide for setting up Kubernetes monitoring in
//! Checkmk.
//!
//! For that, see the [Checkmk documentation].
//!
//! </div>
//!
//! # Checkmk Kubernetes Monitoring
//!
//! Welcome to **the in-cluster component for Checkmk's Kubernetes monitoring.**
//! Some might call this a Kubernetes "agent". As a code-name we call it
//! _rustik_. Actually, not quite: By _rustik_ we refer to _all_ of the
//! in-cluster components used for Checkmk monitoring of a Kubernetes cluster.
//! But you are looking at the documentation for one piece (out of two):
//! `metrics-cache`.
//!
//! `metrics-cache` is the heart of Checkmk's Kubernetes monitoring. It collects
//! information from various sources, namely the Kubernetes API and the
//! `metrics-fetchers` (the "other" component in _rustik_). It takes the
//! collected information and makes it available to Checkmk in the form of
//! pre-generated, pre-computed Checkmk sections (those `<<<foo>>>` things).
//!
//! ## How sections get generated
//!
//! 1. Information enters `metrics-cache` from several sources:
//!    - **The Kubernetes API** (via [watch]): structural data about the cluster
//!      such as which pods are running, numbers of replicas, phases, resource
//!      requests and limits, etc. This information is managed by the use of
//!      [reflectors](kube::runtime::reflector()), a concept native to the
//!      `kube-rs` library on which we depend.
//!    - **The `metrics-fetcher`s**: real-time usage data (how much RAM/CPU is
//!      each container in a pod using? -- from which we can also derive Pod
//!      usage) storage usage and capacity, node health (also runs the Checkmk
//!      openwrt agent... or at least some of it). Each `metrics-fetcher`
//!      pushes into dedicated [`crate::handlers::ingest`] handlers which store
//!      the data in a [`moka::future::Cache`] which lives in
//!      [`crate::state::AppState`].
//!
//! 2. When section generation is triggered (via a request on the pull-mode
//!    endpoint or the push-mode timer), an instantaneous
//!    [`crate::snapshot::Snapshot`] is captured from the data collected in (1).
//!    This means the data does not change out from under us while we are
//!    generating sections, as the data in the caches in-memory can change at
//!    any time. **Once the snapshot is taken, every other step relies on it,
//!    never touching the raw live data.** The frozen API-data stores are
//!    vectors of `Arc<K>` where `K` is the _[kind]_ of data the reflector is
//!    watching. By using an [`std::sync::Arc`] here, we avoid needing to
//!    duplicate every object we are watching, and we ensure it will stay around
//!    in memory until we don't need it anymore.
//!
//!    As the snapshot is made, some internal data maps such as the
//!    [`crate::snapshot::owner_graph::OwnerGraph`] and several
//!    [`crate::snapshot::indexes::Indexes`] are constructed for fast
//!    lookups during section generation. A
//!    [`crate::ingest::reflectors::FrozenStores`] is where the snapshot of each
//!    reflector data store is held in the snapshot.
//!
//! 3. Each kind we monitor is (or _can_ be) represented as a piggyback host.
//!    As such, each has a `struct` representing it in [`crate::piggyback`].
//!    The next step of section generation is that
//!    [`crate::piggyback::emit_all()`] iterates the relevant frozen store and
//!    constructs a piggyback host per object. The constructor of the piggyback
//!    hosts acts as a filter in that it returns an `Option`. As an example,
//!    [`crate::piggyback::namespace::Namespace::new()`] for namespaces returns
//!    `None` for namespaces with no running or pending pod.
//!
//! 4. Each host emits its sections. The trait
//!    [`crate::piggyback::PiggybackHost`] has a single method
//!    [`crate::piggyback::PiggybackHost::emit()`] which returns the list of
//!    sections emitted by each host.
//!
//!    The piggyback host's `emit()` will almost always call
//!    [`crate::section::writeable::WriteableSection::of()`] which is where the
//!    computed section data gets turned into a JSON `String` and stored for
//!    render.
//!
//!    Importantly: Failures degrade gracefully; failure to emit one section
//!    does not affect other sections or ruin the whole output.
//!
//! 5. Lastly [`crate::section::writeable::frame()`] frames the data by grouping
//!    the [`crate::section::writeable::WriteableSection`]s produced by the
//!    piggyback host's `emit()`, and writing it to a writer in section format.
//!
//! [Checkmk documentation]: https://docs.checkmk.com/latest/en/monitoring_kubernetes.html
//! [watch]: https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes
//! [kind]: https://kubernetes.io/docs/reference/using-api/api-concepts/#standard-api-terminology:~:text=All%20resource%20types%20have%20a%20concrete%20representation%20(their%20object%20schema)%20which%20is%20called%20a%20kind

pub mod auth;
pub mod cli_args;
pub mod error;
pub mod handlers;
pub mod host_settings;
pub mod ingest;
pub mod otel;
pub mod piggyback;
pub mod push;
pub mod section;
pub mod snapshot;
pub mod startup;
pub mod state;
#[cfg(test)]
mod test_support;

pub use state::AppState;
