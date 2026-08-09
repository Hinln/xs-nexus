ARG SOURCE_DATE_EPOCH=0
ARG VCS_REF=unknown
ARG XS_VERSION=0.1.0
ARG XS_SOURCE_URL=https://github.com/Hinln/xs-nexus
FROM golang:1.26.5-alpine3.23@sha256:622e56dbc11a8cfe87cafa2331e9a201877271cbff918af53d3be315f3da88cc AS caddy-builder

ARG CADDY_REVISION=e2eee6a7fce366321294c9c2a79f3146891dcbdf
ARG CADDY_CEL_PATCH_REVISION=b2693fb63a30e6d7be0972c3645e9a2c0a500e93

RUN apk add --no-cache git

WORKDIR /src/caddy
RUN git init \
    && git remote add origin https://github.com/caddyserver/caddy.git \
    && git fetch --depth=1 origin "$CADDY_REVISION" \
    && git checkout --detach FETCH_HEAD \
    && test "$(git rev-parse HEAD)" = "$CADDY_REVISION" \
    && git fetch --depth=2 origin "$CADDY_CEL_PATCH_REVISION" \
    && git cherry-pick --no-commit FETCH_HEAD \
    && test "$(go list -m -f '{{.Version}}' github.com/google/cel-go)" = "v0.29.2" \
    && go get go.opentelemetry.io/otel@v1.44.0 \
        golang.org/x/text@v0.39.0 \
        google.golang.org/grpc@v1.82.1 \
    && go mod tidy \
    && CGO_ENABLED=0 go build -trimpath -buildvcs=false \
        -ldflags="-s -w -X github.com/caddyserver/caddy/v2.CustomVersion=v2.11.4-xs1" \
        -o /usr/local/bin/caddy ./cmd/caddy

FROM alpine:3.23@sha256:fd791d74b68913cbb027c6546007b3f0d3bc45125f797758156952bc2d6daf40 AS runtime-rootfs

COPY --from=caddy-builder /usr/local/bin/caddy /usr/bin/caddy
COPY --from=caddy-builder /src/caddy/LICENSE /usr/share/licenses/caddy/LICENSE

RUN apk add --no-cache ca-certificates tzdata libcap spdx-licenses-text \
    && install -d /tmp/xs-spdx /usr/share/licenses/spdx \
    && cp -a /usr/share/spdx/text/. /tmp/xs-spdx/ \
    && if [ -n "$(getcap /usr/bin/caddy)" ]; then setcap -r /usr/bin/caddy; fi \
    && test -z "$(getcap /usr/bin/caddy)" \
    && apk del --no-cache libcap spdx-licenses-text \
    && cp -a /tmp/xs-spdx/. /usr/share/licenses/spdx/ \
    && rm -rf /tmp/xs-spdx \
    && addgroup -g 65532 xs-nexus \
    && adduser -D -H -u 65532 -G xs-nexus xs-nexus \
    && sed -i 's/^xs-nexus:!:[0-9]*:/xs-nexus:!:0:/' /etc/shadow \
    && rm -f /var/log/apk.log

FROM scratch
ARG VCS_REF=unknown
ARG XS_VERSION=0.1.0
ARG XS_SOURCE_URL=https://github.com/Hinln/xs-nexus
COPY --from=runtime-rootfs / /
ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

USER 65532:65532

LABEL org.opencontainers.image.title="XS Nexus HTTPS Edge" \
      org.opencontainers.image.revision="$VCS_REF" \
      org.opencontainers.image.version="$XS_VERSION" \
      org.opencontainers.image.source="$XS_SOURCE_URL"

ENTRYPOINT []
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
