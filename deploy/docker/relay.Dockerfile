ARG SOURCE_DATE_EPOCH=0
ARG VCS_REF=unknown
ARG XS_VERSION=0.1.0
ARG XS_SOURCE_URL=https://github.com/Hinln/xs-nexus
ARG RUST_IMAGE=rust:1.94.0-bookworm@sha256:365468470075493dc4583f47387001854321c5a8583ea9604b297e67f01c5a4f
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12:nonroot@sha256:fccdbb0a547c14e23fcf4ce8ad62ca5d43b4faae8d22cd292f490fef9946c96e

FROM ${RUST_IMAGE} AS builder
ARG SOURCE_DATE_EPOCH
ARG VCS_REF
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
RUN XS_BUILD_GIT_COMMIT="${VCS_REF}" XS_BUILD_DATE_EPOCH="${SOURCE_DATE_EPOCH}" \
    cargo build --locked --release -p xs-relay

FROM ${RUNTIME_IMAGE}
ARG VCS_REF=unknown
ARG XS_VERSION=0.1.0
ARG XS_SOURCE_URL=https://github.com/Hinln/xs-nexus
LABEL org.opencontainers.image.title="XS Nexus Relay" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.version="${XS_VERSION}" \
      org.opencontainers.image.source="${XS_SOURCE_URL}"
COPY --from=builder --chown=65532:65532 /src/target/release/xs-relay /usr/local/bin/xs-relay
USER 65532:65532
EXPOSE 42001/udp 8081/tcp
ENTRYPOINT ["/usr/local/bin/xs-relay"]
