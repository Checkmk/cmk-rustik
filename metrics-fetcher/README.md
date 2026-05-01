# Checkmk Kubernetes `metrics-fetcher`

The `metrics-fetcher` is what collects metrics stores that data in the
`metrics-cache`. In the Python version, this was referred to as the
"node collector".

It can run in two modes:

- `node` - runs the `check_mk_agent` script, reporting information about the
  health of this particular Kubernetes node.
- `containers` - queries [cAdvisor](https://github.com/google/cadvisor) to
  gather and report information about the containers running on this node and.
