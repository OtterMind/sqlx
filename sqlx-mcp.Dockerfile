# SQLX as an MCP server, built from the published Linux release.
#
# The MCP Registry verifies that a publisher owns an OCI package by comparing this label with the
# server name in server.json, so the two must stay identical.
#
# A container reaches only the SQL-execution path. The credential page, the result page and the
# local workbench are all served on 127.0.0.1 inside the container, so a browser on the host cannot
# open them unless the client publishes the port; and the data directory lives in the container, so
# saved connections and downloaded workers are lost unless it is mounted.
FROM ubuntu:24.04

LABEL io.modelcontextprotocol.server.name="io.github.OtterMind/sqlx"

ARG SQLX_VERSION=0.1.21
ARG TARGETARCH

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl unzip; \
    rm -rf /var/lib/apt/lists/*; \
    case "${TARGETARCH}" in \
      amd64) asset=x64 ;; \
      arm64) asset=arm64 ;; \
      *) echo "unsupported arch: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/sqlx.zip "https://github.com/OtterMind/sqlx/releases/download/v${SQLX_VERSION}/sqlx-linux-${asset}.zip"; \
    unzip -q /tmp/sqlx.zip -d /opt/sqlx; \
    rm /tmp/sqlx.zip; \
    ln -sf /opt/sqlx/sqlx /usr/local/bin/sqlx; \
    sqlx --version

# Connections and downloaded components belong in one directory, so a client that wants them
# survives the container by mounting /root/.sqlx.
VOLUME ["/root/.sqlx"]

ENTRYPOINT ["sqlx", "mcp"]
