ARG SOURCE_DATE_EPOCH=0
ARG VCS_REF=unknown
ARG XS_VERSION=0.1.0
ARG XS_SOURCE_URL=https://github.com/Hinln/xs-nexus
ARG NODE_IMAGE=node:24-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03
ARG NGINX_IMAGE=nginxinc/nginx-unprivileged:1.29.8-alpine-slim@sha256:59678856b05324b7f6371f26eb1520be7fcd8bdc8ab380fc4913db8503e5a842

FROM ${NODE_IMAGE} AS builder
ARG SOURCE_DATE_EPOCH
ARG VCS_REF
ARG XS_VERSION
ARG XS_SOURCE_URL
WORKDIR /src
COPY package.json package-lock.json ./
COPY apps/console/package.json ./apps/console/package.json
RUN npm ci --ignore-scripts
COPY apps/console ./apps/console
RUN npm run build \
    && XS_BUILD_GIT_COMMIT="${VCS_REF}" \
       XS_BUILD_DATE_EPOCH="${SOURCE_DATE_EPOCH}" \
       XS_BUILD_VERSION="${XS_VERSION}" \
       XS_BUILD_SOURCE_URL="${XS_SOURCE_URL}" \
       node -e 'const fs=require("node:fs"); const commit=process.env.XS_BUILD_GIT_COMMIT; const epoch=process.env.XS_BUILD_DATE_EPOCH; const version=process.env.XS_BUILD_VERSION; const source=process.env.XS_BUILD_SOURCE_URL; if(commit!=="unknown"&&!/^[0-9a-f]{40}$/.test(commit)) throw new Error("invalid Git commit"); if(!/^[0-9]+$/.test(epoch)) throw new Error("invalid build epoch"); if(!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error("invalid version"); if(!/^https:\/\//.test(source)) throw new Error("invalid source URL"); const value={product:"xs-nexus",component:"xs-console",version,commit,protocol_version:"XSP/1",build_date_epoch:epoch,source}; fs.writeFileSync("/src/apps/console/dist/version.json",JSON.stringify(value)+"\n",{encoding:"utf8",mode:0o644});'

FROM ${NGINX_IMAGE} AS license-materials
USER root
RUN apk add --no-cache spdx-licenses-text

FROM ${NGINX_IMAGE}
ARG VCS_REF=unknown
ARG XS_VERSION=0.1.0
ARG XS_SOURCE_URL=https://github.com/Hinln/xs-nexus
LABEL org.opencontainers.image.title="XS Nexus Console" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.version="${XS_VERSION}" \
      org.opencontainers.image.source="${XS_SOURCE_URL}"
USER root
COPY --from=license-materials /usr/share/spdx/text/ /usr/share/licenses/spdx/
COPY deploy/docker/console.conf.template /etc/nginx/templates/default.conf.template
COPY --from=builder --chown=101:101 /src/apps/console/dist /usr/share/nginx/html
USER 101:101
EXPOSE 8080/tcp
