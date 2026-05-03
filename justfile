docker_tag := "cmk-rustik:1"
helm_release_name := "myrelease"

# Build an image for Kubernetes using Docker
dockerize:
    docker build -t {{docker_tag}} -f docker/Dockerfile .

[private]
run:
    #!/bin/sh
    NODE_NAME=just-hurl \
    HOSTNAME=just-hurl \
    cargo run --bin cmk-rustik-cache-server -- \
    --log-level=warning \
    --address=0.0.0.0 \
    --port=62287 \
    --cache-maxsize=50000 \
    --reader-whitelist=checkmk-monitoring:myrelease-checkmk-checkmk \
    --writer-whitelist=checkmk-monitoring:myrelease-checkmk-node-collector-container-metrics,checkmk-monitoring:myrelease-checkmk-node-collector-machine-sections \
    --cache-ttl=5 &
    echo $! > .pid

# Run the Hurl tests (token retrieved from Kubernetes)
hurl: run
    #!/bin/sh
    trap "kill $(cat .pid) && rm .pid" EXIT
    sleep 1
    READ_TKN=$(kubectl get secret {{helm_release_name}}-checkmk-checkmk \
        -n checkmk-monitoring \
        -o=jsonpath='{.data.token}' \
        | base64 --decode)
    WRITE_TKN=$(kubectl \
        -n checkmk-monitoring \
        exec -it \
        daemonsets/{{helm_release_name}}-checkmk-node-collector-machine-sections \
        -- cat /var/run/secrets/kubernetes.io/serviceaccount/token)
    hurl --test metrics-cache/tests/hurl \
    --variable baseurl=http://127.0.0.1:62287 \
    --variable write_token=$WRITE_TKN \
    --variable read_token=$READ_TKN
