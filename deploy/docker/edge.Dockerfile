FROM caddy:2.10.2-alpine@sha256:d8c17a862962def15cde69863a3a463f25a2664942eafd7bdbf050e9c3116b83

ARG VCS_REF=unknown

RUN apk upgrade --no-cache \
    && apk add --no-cache libcap \
    && setcap -r /usr/bin/caddy \
    && test -z "$(getcap /usr/bin/caddy)" \
    && apk del --no-cache libcap \
    && addgroup -g 65532 xs-nexus \
    && adduser -D -H -u 65532 -G xs-nexus xs-nexus

USER 65532:65532

LABEL org.opencontainers.image.title="XS Nexus HTTPS Edge" \
      org.opencontainers.image.revision="$VCS_REF"

ENTRYPOINT []
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
