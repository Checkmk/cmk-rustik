# Checkmk Kubernetes `metrics-cache`

The `metrics-cache` is where the `metrics-fetcher` sends its data. In the Python
code, this was referred to as the "cluster collector".

The data can be accessed by Checkmk pulling it (pull mode) from the
`metrics-cache` or by the `metrics-cache` pushing the data into Checkmk (push
mode). (NOTE: push mode is not yet supported)

### Background

Normally, Checkmk talks directly to the Kubernetes API _and also_ some other
service to enrich the data returned from the API. That "other service" can be
Prometheus/Thanos in the case of OpenShift, or a "cluster collector" like
this one (or the older Python version).

The `metrics-fetcher` (see its `README.md` for more information) sends its data
to us (the `metrics-cache`). The `metrics-cache` can then serve that data via
HTTP (JSON) when asked or push the data into Checkmk (when supported).
