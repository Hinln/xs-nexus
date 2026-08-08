ARG SOURCE_DATE_EPOCH=0
ARG NODE_IMAGE=node:24-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03
ARG NGINX_IMAGE=nginxinc/nginx-unprivileged:1.29.8-alpine-slim@sha256:59678856b05324b7f6371f26eb1520be7fcd8bdc8ab380fc4913db8503e5a842

FROM ${NODE_IMAGE} AS builder
WORKDIR /src
COPY package.json package-lock.json ./
COPY apps/console/package.json ./apps/console/package.json
RUN npm ci --ignore-scripts
COPY apps/console ./apps/console
RUN npm run build

FROM ${NGINX_IMAGE} AS license-materials
USER root
RUN apk add --no-cache spdx-licenses-text

FROM ${NGINX_IMAGE}
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="XS Nexus Console" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.source="XS Nexus clean-room repository"
USER root
COPY --from=license-materials /usr/share/spdx/text/ /usr/share/licenses/spdx/
COPY deploy/docker/console.conf.template /etc/nginx/templates/default.conf.template
COPY --from=builder --chown=101:101 /src/apps/console/dist /usr/share/nginx/html
USER 101:101
EXPOSE 8080/tcp
