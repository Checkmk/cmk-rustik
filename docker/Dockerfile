FROM rust:1.95-alpine3.23 AS builder
WORKDIR /src
COPY . .
RUN cargo install --path ./metrics-cache && \
    cargo install --path ./metrics-fetcher

FROM docker.io/checkmk/check-mk-community:2.5.0-daily AS checkmk

FROM alpine:3.23
ARG CHECKMK_AGENT_DIR="/opt/omd/versions/default/share/check_mk/agents"
ARG CHECKMK_AGENT_PLUGIN_DIR="${CHECKMK_AGENT_DIR}/plugins"
COPY --from=checkmk "${CHECKMK_AGENT_DIR}/check_mk_agent.openwrt" /install/bin/check_mk_agent.openwrt
COPY --from=checkmk "${CHECKMK_AGENT_PLUGIN_DIR}/mk_inventory.linux" /install/bin/mk_inventory.linux
COPY --from=checkmk "${CHECKMK_AGENT_PLUGIN_DIR}/lnx_container_host_if.linux" /install/bin/lnx_container_host_if.linux
COPY --from=builder /usr/local/cargo/bin/cmk-rustik-cache-server /usr/local/bin/
COPY --from=builder /usr/local/cargo/bin/metrics-fetcher /usr/local/bin/
COPY docker/check_mk_agent /usr/local/bin/check_mk_agent
RUN addgroup appgroup -g 10001 && \
    adduser -HD appuser -G appgroup -u 10001 && \
    # compat with what the Helm chart currently expects
    ln -s /usr/local/bin/cmk-rustik-cache-server /usr/local/bin/checkmk-cluster-collector
USER appuser
