FROM rust:1.96-alpine3.23 AS base
RUN cargo install cargo-chef --version ^0.1

FROM base AS planner
WORKDIR /src
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /src
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --target "$(uname -m)-unknown-linux-musl"
COPY . .
RUN cargo build --release --target "$(uname -m)-unknown-linux-musl"

FROM --platform=linux/amd64 docker.io/checkmk/check-mk-community:2.5.0-daily AS checkmk

FROM alpine:3.23 AS metrics-fetcher
ARG CHECKMK_AGENT_DIR="/opt/omd/versions/default/share/check_mk/agents"
ARG CHECKMK_AGENT_PLUGIN_DIR="${CHECKMK_AGENT_DIR}/plugins"
COPY --from=checkmk "${CHECKMK_AGENT_DIR}/check_mk_agent.openwrt" /install/bin/check_mk_agent.openwrt
COPY --from=checkmk "${CHECKMK_AGENT_PLUGIN_DIR}/mk_inventory.linux" /install/bin/mk_inventory.linux
COPY --from=checkmk "${CHECKMK_AGENT_PLUGIN_DIR}/lnx_container_host_if.linux" /install/bin/lnx_container_host_if.linux
COPY --from=builder /src/target/*/release/metrics-fetcher /usr/local/bin/
COPY docker/check_mk_agent /usr/local/bin/check_mk_agent
RUN addgroup appgroup -g 10001 && adduser -HD appuser -G appgroup -u 10001
USER appuser
ENV PATH="/src/target/debug:/src/target/x86_64-unknown-linux-musl/debug:/src/target/aarch64-unknown-linux-musl/debug:${PATH}"
ENTRYPOINT ["metrics-fetcher"]

FROM alpine:3.23 AS metrics-cache
COPY --from=builder /src/target/*/release/metrics-cache /usr/local/bin/
RUN addgroup appgroup -g 10001 && adduser -HD appuser -G appgroup -u 10001
USER appuser
ENV PATH="/src/target/debug:/src/target/x86_64-unknown-linux-musl/debug:/src/target/aarch64-unknown-linux-musl/debug:${PATH}"
ENTRYPOINT ["metrics-cache"]
