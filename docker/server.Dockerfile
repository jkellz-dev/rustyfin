# Runtime packages come from the base image (docker/base.Dockerfile, target
# `server`); CI passes the exact tag it built or found. The `server-latest`
# default is for local builds.
ARG BASE_IMAGE=ghcr.io/jkellz-dev/rustyfin-base:server-latest
FROM ${BASE_IMAGE}

ARG TARGETARCH

COPY linux/${TARGETARCH}/rustyfin-server /usr/local/bin/rustyfin-server
COPY migrations /app/migrations

WORKDIR /app

EXPOSE 8096

ENTRYPOINT ["rustyfin-server"]
