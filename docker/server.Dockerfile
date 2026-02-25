FROM debian:bookworm-slim

ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY linux/${TARGETARCH}/rustyfin-server /usr/local/bin/rustyfin-server
COPY migrations /app/migrations

WORKDIR /app

EXPOSE 8096

ENTRYPOINT ["rustyfin-server"]
