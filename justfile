fetcher_tag := "cmk-rustik-metrics-fetcher:local"
fetcher_target := "metrics-fetcher-dev"

cache_tag := "cmk-rustik-metrics-cache:local"
cache_target := "metrics-cache-dev"

# Build an image for Kubernetes using Docker
dockerize:
    docker build -t {{fetcher_tag}} --target {{fetcher_target}} -f docker/Dockerfile .
    docker build -t {{cache_tag}} --target {{cache_target}} -f docker/Dockerfile .

# Create a kind cluster for development
kind-create:
    sed "s#\\\$SRC_DIR\\\$#$(pwd)#" devel/kind-config.yaml | \
      kind create cluster --name rustik --config -

# Load images into the kind cluster, creating it if it does not exist
kind-load: dockerize kind-create
    kind load docker-image {{fetcher_tag}} --name rustik
    kind load docker-image {{cache_tag}} --name rustik

# Load the helm chart into the kind cluster with devel/values.yaml
kind-helm-install: kind-load
    helm upgrade --install rustik ./charts/cmk-rustik -n checkmk-monitoring \
      --create-namespace -f devel/values.yaml

# DEV ENV: Deploy rustik in Kind with source mounted at /src
kind-dev: dockerize kind-create kind-load kind-helm-install

# Remove the Kind dev cluster
kind-dev-teardown:
    kind delete cluster --name rustik
