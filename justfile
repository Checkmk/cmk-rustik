fetcher_tag := "cmk-rustik-metrics-fetcher:local"
cache_tag := "cmk-rustik-metrics-cache:local"

# Build an image for Kubernetes using Docker
dockerize:
    docker build -t {{fetcher_tag}} --target metrics-fetcher -f docker/Dockerfile .
    docker build -t {{cache_tag}} --target metrics-cache -f docker/Dockerfile .

# Deploy rustik in Kind with source mounted at /src
kind-dev: dockerize
    sed "s#\\\$SRC_DIR\\\$#$(pwd)#" k8s/kind-config.yaml | \
      kind create cluster --name rustik --config -
    kind load docker-image {{fetcher_tag}} --name rustik
    kind load docker-image {{cache_tag}} --name rustik
    kubectl apply -f k8s/dev-deployment.yaml
    kubectl apply -f k8s/dev-daemonset.yaml

# Remove the Kind dev cluster
kind-dev-teardown:
    kind delete cluster --name rustik
