ARG RUST_IMAGE=rust:1.93.0-bookworm
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12:nonroot@sha256:fccdbb0a547c14e23fcf4ce8ad62ca5d43b4faae8d22cd292f490fef9946c96e

FROM ${RUST_IMAGE} AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
RUN cargo build --locked --release -p xs-controller

FROM ${RUNTIME_IMAGE}
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="XS Nexus Controller" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.source="XS Nexus clean-room repository"
COPY --from=builder --chown=65532:65532 /src/target/release/xs-controller /usr/local/bin/xs-controller
USER 65532:65532
EXPOSE 8080/tcp 42000/udp
ENTRYPOINT ["/usr/local/bin/xs-controller"]
CMD ["serve"]
