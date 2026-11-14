# Runtime packages come from the base image (docker/base.Dockerfile, target
# `meta`); CI passes the exact tag it built or found. The `meta-latest` default
# is for local builds.
ARG BASE_IMAGE=ghcr.io/jkellz-dev/rustyfin-base:meta-latest
FROM ${BASE_IMAGE}

ARG TARGETARCH

COPY linux/${TARGETARCH}/rustyfin /usr/local/bin/rustyfin

ENTRYPOINT ["rustyfin"]
