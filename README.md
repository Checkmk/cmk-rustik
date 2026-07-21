# cmk-rustik

An in-cluster Kubernetes monitoring agent for Checkmk built in Rust.

Supports push-mode, pull-mode, and OpenTelemetry egress to Checkmk.

Under the hood, rustik is two components: *metrics-fetcher*, a `DaemonSet`
which scrapes the Kubelet on every node for metrics (and runs the Checkmk
Linux agent), and *metrics-cache*, a `Deployment` which watches the Kubernetes
API, caches what the fetchers send it, and joins it all together into
ready-to-go Checkmk sections.

Note that this is a **work in progress**, but we gladly welcome early testers
and contributions! It's also changing fast and not yet stable, so please don't
actually rely on it for anything important yet!

Interested in what we are doing? Grab a
[daily build](https://checkmk.com/download/archive#checkmk-dailies) of Checkmk
and a copy of this repo and
[try rustik in your cluster](https://github.com/checkmk/cmk-rustik/wiki/Testing-on-an-unmodified-Checkmk).
Later in this `README.md`, you will find a list of dependencies to get the
development environment spun up.

Alternatively, we build images on each push to `master` and have a rolling
`0.0.0-master` release of our Helm chart. You can give it a try with:

```bash
helm install rustik oci://ghcr.io/checkmk/charts/cmk-rustik --version 0.0.0-master \
    --set clusterName=mycluster \
    --set clusterHostName=my-cmk-host \
    --set push.enabled=true \
    --set push.registrationToken=0:e07f1760-d9de-4bb0-a55e-a0fcc4e8355f \
    --set push.url=https://your-checkmk.example.com:8000/yoursite \
    -n checkmk-monitoring \
    --create-namespace
```

Of course, note that push mode and OpenTelemetry will only work in Checkmk
Ultimate or higher.

## Building / Dev Environment

A quick list of dependencies you will likely need to build rustik:

* A musl toolchain, since we target Alpine (`musl-tools` on Debian-ish)
* A Rust toolchain (we target stable, not nightly). Also helpful to have a few
  components like `clippy` and `rustfmt`. Also `rust-analyzer` if you want
  LSP/IDE integration
* [just](https://github.com/casey/just) as our command runner of choice
* [kind](https://kind.sigs.k8s.io/) for local testing (the dev env assumes it)
* [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/)
  (preferably with working shell-completion for your shell; personally I alias
  `kc` to `kubectl -n checkmk-monitoring` and then use
  [complete-alias](https://github.com/cykerway/complete-alias) to get completion
  on the alias)
* [helm](https://helm.sh/) for the Helm chart (the dev env assumes it)
* (optional) [kubeconform](https://github.com/yannh/kubeconform)
  (`just lint-helm` assumes it)
* (optional) [chart-testing](https://github.com/helm/chart-testing)
  (`just lint-helm` assumes it; move its `etc/*` files into `~/.ct/`)
* (optional, but not really)
  [cargo-insta](https://insta.rs/docs/cli/) for reviewing snapshot test
  changes. You'll want it the first time a snapshot test fails on you.
* (optional) A Checkmk instance running to see things working.

Initial `cargo build` will be slow, after that it should be fast unless deps
change.

Run the tests with `cargo test`. We make heavy use of
[insta](https://insta.rs/) snapshot tests, so if you intentionally change
section output, run `cargo insta review` to look at and accept the new
snapshots.

As another optional dependency to make `cargo build` link our own binaries
slightly faster, consider installing the linker `mold` and setting this in your
`~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-musl]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

### Starting the dev environment

(Do this _after_ you have a successful build with `cargo build`)

Spin up the dev environment with **`just kind-dev`**. This will:
* Build a dev image (does _not_ compile rustik in Docker like the images from CI
  do - it's too slow in the dev env)
* Spin up a kind cluster with a specific config (`devel/kind-config.yaml` + a
  `sed` to point it to the source directory).
* Kind will mount your source directory into the Docker container running
  Kubernetes, and then these can be mounted into the containers that Kubernetes
  spawns.
* Install the Helm chart from a set of development defaults
  (`devel/values.yaml`), which configure the source mount mentioned in the
  previous point.

If you are iterating on the Helm chart, you can `just kind-helm-delete` and
`just kind-helm-install`. For the targets which install the Helm chart
(such as `kind-helm-install` but also `kind-dev`), you can pass in several
overrides, such as
`just push=true push_ott=0:e07f1760-d9de-4bb0-a55e-a0fcc4e8355f kind-dev`.
See the variables at the top of the `justfile` for options.

`just kind-dev-teardown` will tear down the kind cluster and everything in it.

`just sanity` will run a rough subset of what the CI workflows run. It is not a
complete replication of CI, but in many cases it will find issues before you
have to wait for the full CI run. If you use pre-commit hooks, this could be a
good command to consider running from one of them.

`just -l` gives you a list of all available recipes. But the most common and
useful ones are the ones mentioned above.

## License

GPL-2.0-only
