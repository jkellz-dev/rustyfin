# Self-Hosted MusicBrainz

Run a local MusicBrainz instance for unlimited metadata lookups without rate limiting.

## Quick Start

```bash
./setup.sh
```

This clones the [official musicbrainz-docker](https://github.com/metabrainz/musicbrainz-docker) project and runs its setup process.

## Usage

```bash
# Clone and set up from scratch
./setup.sh

# Use an existing musicbrainz-docker clone
./setup.sh /path/to/musicbrainz-docker
```

## Requirements

- Docker and Docker Compose
- ~50 GB disk space (database dump + PostgreSQL data)
- 4+ GB RAM

## Configuration

After setup, configure rustyfin to use the local instance:

```toml
# rustyfin-meta.toml
[musicbrainz]
base_url = "http://localhost:5000/ws/2"
```

The local instance is automatically detected (no rate limiting applied).

## Maintenance

The official musicbrainz-docker setup includes daily replication to keep the database current. See the [upstream documentation](https://github.com/metabrainz/musicbrainz-docker) for details.

## Standalone Docker Compose

A simplified `docker-compose.yml` is provided for reference. For production use, prefer the official setup script which handles database import and replication.
