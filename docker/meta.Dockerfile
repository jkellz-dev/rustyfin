FROM debian:bookworm-slim

ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY linux/${TARGETARCH}/rustyfin /usr/local/bin/rustyfin

ENTRYPOINT ["rustyfin"]
