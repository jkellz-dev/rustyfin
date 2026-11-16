#!/bin/sh
# Generates the synthetic media set the parity suite scans on both servers.
# A few seconds each, tiny, so every run starts from identical files.
# Usage: make_media.sh <output-dir>
set -eu

out="${1:?usage: make_media.sh <output-dir>}"
mkdir -p "$out/movies/Test Movie (2020)" \
	"$out/shows/Test Show (2021)/Season 01" \
	"$out/music/Test Artist/Test Album (2019)"

# Picard-style MusicBrainz tags (TXXX frames), so both servers read the same
# ids from the files: the ids Jellyfin's own online lookup found for this album.
MB_ALBUM=43bcfb95-f26c-4f8d-84f8-7b2ac5b8ab72
MB_GROUP=5b35fbbc-3f45-45a8-95a9-2545dbf44e7c
MB_ARTIST=7e84f845-ac16-41fe-9ff8-df12eb32af55

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
		-metadata album="Test Album" -metadata track="$4" \
		-metadata "MusicBrainz Album Id=$MB_ALBUM" \
		-metadata "MusicBrainz Release Group Id=$MB_GROUP" \
		-metadata "MusicBrainz Artist Id=$MB_ARTIST" \
		-metadata "MusicBrainz Album Artist Id=$MB_ARTIST" "$1"
}

video "$out/movies/Test Movie (2020)/Test Movie (2020).mp4" "Test Movie"
video "$out/shows/Test Show (2021)/Season 01/Test Show S01E01.mp4" "Pilot"
video "$out/shows/Test Show (2021)/Season 01/Test Show S01E02.mp4" "Second"
audio "$out/music/Test Artist/Test Album (2019)/01 First.mp3" "First" 330 1
audio "$out/music/Test Artist/Test Album (2019)/02 Second.mp3" "Second" 550 2

# Lyrics (11a): Jellyfin reads <track>.lrc (synced) and <track>.txt (plain).
printf '[ar:Test Artist]\n[ti:First]\n[00:00.50]Line one\n[00:01.75]Line two\n\n[00:03.00][00:04.00]Twice\n' \
	>"$out/music/Test Artist/Test Album (2019)/01 First.lrc"
printf 'Plain one\n\nPlain two\n' >"$out/music/Test Artist/Test Album (2019)/02 Second.txt"

find "$out" -type f -exec touch -d "2021-01-01 00:00:00" {} +
echo "media written to $out"
