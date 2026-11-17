#!/bin/sh
# Writes a synthetic library (~80,000 items: 2,700 films,
# 300 shows with 3,600 episodes, 1,500 artists with 74,000 tracks) as empty
# files with real names, for the 11b/11c/11d stress test. Empty files are not
# playable; the scan records them (probe and tag reads fail softly), which is
# what the stress test measures: walking, naming, the database and the API at
# this size. Tens of seconds to write.
#
#   sh tests/stress/make_library.sh <dir>
set -eu
out=${1:?output directory}
films=${FILMS:-2700}
shows=${SHOWS:-300}
artists=${ARTISTS:-1500}
albums=${ALBUMS:-5}
tracks=${TRACKS:-10}

mkdir -p "$out/movies" "$out/shows" "$out/music"

i=1
while [ "$i" -le "$films" ]; do
	year=$((1950 + i % 75))
	d="$out/movies/Film $i ($year)"
	mkdir -p "$d"
	: >"$d/Film $i ($year).mkv"
	# One film in ten carries a trailer and a featurette (11j).
	if [ $((i % 10)) -eq 0 ]; then
		: >"$d/Film $i ($year)-trailer.mkv"
		mkdir -p "$d/featurettes"
		: >"$d/featurettes/Making Of.mkv"
	fi
	i=$((i + 1))
done

s=1
while [ "$s" -le "$shows" ]; do
	seasons=$((1 + s % 6))
	n=1
	while [ "$n" -le "$seasons" ]; do
		d="$out/shows/Show $s ($((1990 + s % 30)))/Season $(printf %02d "$n")"
		mkdir -p "$d"
		eps=$((2 + (s + n) % 3))
		e=1
		while [ "$e" -le "$eps" ]; do
			: >"$d/Show $s S$(printf %02d "$n")E$(printf %02d "$e").mkv"
			e=$((e + 1))
		done
		n=$((n + 1))
	done
	s=$((s + 1))
done

a=1
while [ "$a" -le "$artists" ]; do
	b=1
	while [ "$b" -le "$albums" ]; do
		d="$out/music/Artist $a/Album $b ($((1960 + (a + b) % 60)))"
		mkdir -p "$d"
		t=1
		while [ "$t" -le "$tracks" ]; do
			: >"$d/$(printf %02d "$t") Track $t.mp3"
			t=$((t + 1))
		done
		b=$((b + 1))
	done
	a=$((a + 1))
done

echo "files: $(find "$out" -type f | wc -l)"
