ARG RUST_IMAGE=rust:1.93.0-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
RUN cargo build --locked --release -p xs-relay

FROM ${RUNTIME_IMAGE}
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="XS Nexus Relay" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.source="XS Nexus clean-room repository"
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 65532 xs-nexus \
    && useradd --uid 65532 --gid 65532 --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin xs-nexus
COPY --from=builder --chown=65532:65532 /src/target/release/xs-relay /usr/local/bin/xs-relay
USER 65532:65532
EXPOSE 42001/udp 8081/tcp
ENTRYPOINT ["/usr/local/bin/xs-relay"]
