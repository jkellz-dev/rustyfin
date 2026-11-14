# The runtime layers both images share: Debian plus the packages the binaries
# need. Built and pushed by CI only when this file changes (and once a month,
# for Debian security updates), as ghcr.io/jkellz-dev/rustyfin-base:<target>-<key>.
# server.Dockerfile and meta.Dockerfile start from it and only copy binaries in,
# so a normal push installs nothing and needs no arm64 emulation.

# rustyfin (the metadata CLI): TLS roots for the provider APIs.
FROM debian:bookworm-slim AS meta

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# rustyfin-server: also ffmpeg/ffprobe for probing and transcoding.
FROM meta AS server

RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*
