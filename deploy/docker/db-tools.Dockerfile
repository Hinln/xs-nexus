FROM golang:1.25.5-alpine3.22@sha256:3587db7cc96576822c606d119729370dbf581931c5f43ac6d3fa03ab4ed85a10 AS age-builder
ARG AGE_COMMIT=b8564adb6d58329b8a3e267360ca2b0abc4efe1d
ARG AGE_VERSION=v1.3.1
ARG X_CRYPTO_VERSION=v0.52.0
RUN apk add --no-cache git
WORKDIR /src/age
RUN git init \
    && git remote add origin https://github.com/FiloSottile/age.git \
    && git fetch --depth=1 origin "${AGE_COMMIT}" \
    && git checkout --detach FETCH_HEAD \
    && test "$(git rev-parse HEAD)" = "${AGE_COMMIT}" \
    && go get "golang.org/x/crypto@${X_CRYPTO_VERSION}" \
    && test "$(go list -m -f '{{.Version}}' golang.org/x/crypto)" = "${X_CRYPTO_VERSION}" \
    && CGO_ENABLED=0 go build -trimpath -buildvcs=false \
        -ldflags="-s -w -X main.Version=${AGE_VERSION}-xs1" \
        -o /usr/local/bin/age ./cmd/age \
    && CGO_ENABLED=0 go build -trimpath -buildvcs=false \
        -ldflags="-s -w -X main.Version=${AGE_VERSION}-xs1" \
        -o /usr/local/bin/age-keygen ./cmd/age-keygen \
    && install -D -m 0644 LICENSE /usr/share/licenses/age/LICENSE

FROM postgres:18-alpine3.22
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="XS Nexus PostgreSQL Operations" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.source="XS Nexus clean-room repository"
RUN apk upgrade --no-cache \
    && apk add --no-cache bash coreutils spdx-licenses-text \
    && install -d /tmp/xs-spdx /usr/share/licenses/spdx \
    && cp -a /usr/share/spdx/text/. /tmp/xs-spdx/ \
    && apk del --no-cache spdx-licenses-text \
    && cp -a /tmp/xs-spdx/. /usr/share/licenses/spdx/ \
    && rm -rf /tmp/xs-spdx \
    && addgroup -g 65532 xs-nexus \
    && adduser -D -H -u 65532 -G xs-nexus xs-nexus
COPY --from=age-builder /usr/local/bin/age /usr/local/bin/age
COPY --from=age-builder /usr/local/bin/age-keygen /usr/local/bin/age-keygen
COPY --from=age-builder /usr/share/licenses/age /usr/share/licenses/age
COPY --chown=65532:65532 deploy/docker/db-tools.sh /usr/local/bin/xs-db-tools
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/xs-db-tools"]
