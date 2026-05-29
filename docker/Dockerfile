FROM rust:1.96-alpine3.23 AS base
RUN cargo install cargo-chef --version ^0.1

FROM base AS planner
WORKDIR /src
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /src
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM docker.io/checkmk/check-mk-community:2.5.0-daily AS checkmk

FROM alpine:3.23
ARG CHECKMK_AGENT_DIR="/opt/omd/versions/default/share/check_mk/agents"
ARG CHECKMK_AGENT_PLUGIN_DIR="${CHECKMK_AGENT_DIR}/plugins"
COPY --from=checkmk "${CHECKMK_AGENT_DIR}/check_mk_agent.openwrt" /install/bin/check_mk_agent.openwrt
COPY --from=checkmk "${CHECKMK_AGENT_PLUGIN_DIR}/mk_inventory.linux" /install/bin/mk_inventory.linux
COPY --from=checkmk "${CHECKMK_AGENT_PLUGIN_DIR}/lnx_container_host_if.linux" /install/bin/lnx_container_host_if.linux
COPY --from=builder /src/target/*/release/cmk-rustik-cache-server /usr/local/bin/
COPY --from=builder /src/target/*/release/metrics-fetcher /usr/local/bin/
COPY docker/check_mk_agent /usr/local/bin/check_mk_agent
RUN addgroup appgroup -g 10001 && \
    adduser -HD appuser -G appgroup -u 10001 && \
    # compat with what the Helm chart currently expects
    ln -s /usr/local/bin/cmk-rustik-cache-server /usr/local/bin/checkmk-cluster-collector
USER appuser
