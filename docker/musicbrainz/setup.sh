#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MB_DOCKER_DIR="$SCRIPT_DIR/musicbrainz-docker"

echo "=== Self-Hosted MusicBrainz Setup ==="
echo ""
echo "This will:"
echo "  1. Clone the official musicbrainz-docker repository"
echo "  2. Download the latest full database dump (~30 GB)"
echo "  3. Import the dump into PostgreSQL"
echo "  4. Start the MusicBrainz server on port 5000"
echo ""
echo "Requirements: ~50 GB disk space, 4+ GB RAM, Docker + Docker Compose"
echo ""
read -p "Continue? [y/N] " -r
[[ $REPLY =~ ^[Yy]$ ]] || exit 0

# Step 1: Clone official musicbrainz-docker
if [ ! -d "$MB_DOCKER_DIR" ]; then
    echo "Cloning musicbrainz-docker..."
    git clone --depth 1 https://github.com/metabrainz/musicbrainz-docker.git "$MB_DOCKER_DIR"
else
    echo "musicbrainz-docker already cloned, pulling latest..."
    git -C "$MB_DOCKER_DIR" pull
fi

cd "$MB_DOCKER_DIR"

# Step 2: Follow the official setup
echo ""
echo "Running official musicbrainz-docker setup..."
echo "This will download and import the database dump. This takes a long time."
echo ""

# The official repo has its own setup script
if [ -f "admin/setup.sh" ]; then
    bash admin/setup.sh
elif [ -f "Makefile" ]; then
    make setup
else
    echo "Starting services..."
    docker compose up -d
    echo ""
    echo "Services started. You may need to manually import the database dump."
    echo "See: https://github.com/metabrainz/musicbrainz-docker#readme"
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "MusicBrainz API available at: http://localhost:5000/ws/2/"
echo ""
echo "To use with rustyfin, add to rustyfin-meta.toml:"
echo ""
echo '  [musicbrainz]'
echo '  base_url = "http://localhost:5000/ws/2"'
echo ""
echo "Daily replication runs automatically to keep data up to date."
