#!/bin/sh
# Generates the synthetic media set the parity suite scans on both servers.
# A few seconds each, tiny, so every run starts from identical files.
# Usage: make_media.sh <output-dir>
set -eu

out="${1:?usage: make_media.sh <output-dir>}"
mkdir -p "$out/movies/Test Movie (2020)" \
	"$out/shows/Test Show (2021)/Season 01" \
	"$out/music/Test Artist/Test Album (2019)"

# Fixed timestamps so the etag and DateCreated do not differ between runs.
video() {
	ffmpeg -y -loglevel error \
		-f lavfi -i "testsrc=duration=4:size=320x180:rate=24" \
		-f lavfi -i "sine=frequency=440:duration=4" \
		-c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -shortest \
		-metadata title="$2" "$1"
}
audio() {
	ffmpeg -y -loglevel error \
		-f lavfi -i "sine=frequency=$3:duration=4" \
		-c:a libmp3lame -b:a 64k \
		-metadata title="$2" -metadata artist="Test Artist" \
		-metadata album="Test Album" -metadata track="$4" "$1"
}

video "$out/movies/Test Movie (2020)/Test Movie (2020).mp4" "Test Movie"
video "$out/shows/Test Show (2021)/Season 01/Test Show S01E01.mp4" "Pilot"
video "$out/shows/Test Show (2021)/Season 01/Test Show S01E02.mp4" "Second"
audio "$out/music/Test Artist/Test Album (2019)/01 First.mp3" "First" 330 1
audio "$out/music/Test Artist/Test Album (2019)/02 Second.mp3" "Second" 550 2

find "$out" -type f -exec touch -d "2021-01-01 00:00:00" {} +
echo "media written to $out"
