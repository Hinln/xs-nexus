FROM postgres:18-alpine3.22
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="XS Nexus PostgreSQL Operations" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.source="XS Nexus clean-room repository"
RUN apk upgrade --no-cache \
    && apk add --no-cache bash coreutils spdx-licenses-text \
    && apk add --no-cache \
        --repository https://dl-cdn.alpinelinux.org/alpine/edge/community \
        age=1.3.1-r6 \
    && install -d /tmp/xs-spdx /usr/share/licenses/spdx \
    && cp -a /usr/share/spdx/text/. /tmp/xs-spdx/ \
    && apk del --no-cache spdx-licenses-text \
    && cp -a /tmp/xs-spdx/. /usr/share/licenses/spdx/ \
    && rm -rf /tmp/xs-spdx \
    && addgroup -g 65532 xs-nexus \
    && adduser -D -H -u 65532 -G xs-nexus xs-nexus
COPY --chown=65532:65532 deploy/docker/db-tools.sh /usr/local/bin/xs-db-tools
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/xs-db-tools"]
