docker_tag := "cmk-rustik:1"

# Build an image for Kubernetes using Docker
dockerize:
    docker build -t {{docker_tag}} -f docker/Dockerfile .

# Deploy rustik in Kind with source mounted at /src
kind-dev: dockerize
    sed "s#\\\$SRC_DIR\\\$#$(pwd)#" k8s/kind-config.yaml | \
      kind create cluster --name rustik --config -
    kind load docker-image cmk-rustik:1 --name rustik
    kubectl apply -f k8s/dev-deployment.yaml
    kubectl apply -f k8s/dev-daemonset.yaml

# Remove the Kind dev cluster
kind-dev-teardown:
    kind delete cluster --name rustik
