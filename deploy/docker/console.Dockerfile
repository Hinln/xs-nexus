ARG SOURCE_DATE_EPOCH=0
ARG NODE_IMAGE=node:24-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03
ARG NGINX_IMAGE=nginxinc/nginx-unprivileged:1.29-alpine@sha256:0c79d56aee561a1d81c63f00eee5fb5fe29279560cdc55e91425133104c7fbe6

FROM ${NODE_IMAGE} AS builder
WORKDIR /src
COPY package.json package-lock.json ./
COPY apps/console/package.json ./apps/console/package.json
RUN npm ci --ignore-scripts
COPY apps/console ./apps/console
RUN npm run build

FROM ${NGINX_IMAGE}
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="XS Nexus Console" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.source="XS Nexus clean-room repository"
USER root
RUN apk add --no-cache spdx-licenses-text \
    && install -d /tmp/xs-spdx /usr/share/licenses/spdx \
    && cp -a /usr/share/spdx/text/. /tmp/xs-spdx/ \
    && apk del --no-cache spdx-licenses-text \
    && cp -a /tmp/xs-spdx/. /usr/share/licenses/spdx/ \
    && rm -rf /tmp/xs-spdx \
    && apk del --no-cache curl nginx-module-image-filter
COPY deploy/docker/console.conf.template /etc/nginx/templates/default.conf.template
COPY --from=builder --chown=101:101 /src/apps/console/dist /usr/share/nginx/html
USER 101:101
EXPOSE 8080/tcp
