ARG NODE_IMAGE=node:24-bookworm-slim
ARG NGINX_IMAGE=nginxinc/nginx-unprivileged:1.29-alpine

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
RUN apk upgrade --no-cache \
    && apk del --no-cache curl
COPY deploy/docker/console.conf.template /etc/nginx/templates/default.conf.template
COPY --from=builder --chown=101:101 /src/apps/console/dist /usr/share/nginx/html
USER 101:101
EXPOSE 8080/tcp
