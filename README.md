# cmk-rustik

A currently-unofficial re-implementation of
[checkmk_kube_agent](https://github.com/Checkmk/checkmk_kube_agent/) in Rust.

**Currently a work-in-progress/experiment.**

Right now a focus is matching the Python implementation pretty-darn-close to
1:1. This means: not modifying the "wire protocols" (i.e. the JSON schemas)
sent and expected.

This makes each individual component drop-in.

## Components

#### `metrics-cache` _(aka: `cluster_collector` in the Python original)_

This is effectively an in-memory cache server that functions over HTTP and
authenticates requests against the Kubernetes cluster in which it is running.

Because the cache is embedded (not some external service), **one instance**
is expected to be running per cluster. (In the future, we could allow for an
external backing cache or some kind of tiered L1/L2 layered caching and then
permit multiple instances of `metrics-cache` to run for HA.)

Requests are expected to have a ServiceAccount token as a Bearer token. This is
validated by Kubernetes (internally, a `TokenReview` is sent to Kubernetes and
the result is validated and then checked against the allowlist which is
configured by `metrics-cache`'s CLI arguments).

There are/will be three internally-maintained caches in total:

* Container metrics (for metrics coming from cAdvisor)
* Machine sections (for storing a copy of the `check_mk_agent` output to gather
  *node* information)
* `metrics-fetcher` metadata (for storing metadata about the `metics-fetcher`
  instances sending data to the `metrics-cache`).

#### `metrics-fetcher` _(aka: `node_collector` in the Python original)_

A "metrics fetcher", as the name suggests, fetches metrics from some source. It
then forwards those to the metrics cache instance for consumption by Checkmk.

The metrics fetcher is one binary that can be run in two different modes:

* "cAdvisor mode" - fetch container metrics from a cAdvisor instance
* "cmk-agent mode" - run and collect sections from a `check_mk_agent` script


## License

GPLv2, same as the Python original.
