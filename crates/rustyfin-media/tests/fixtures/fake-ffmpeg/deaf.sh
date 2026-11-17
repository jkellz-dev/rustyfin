#!/bin/sh
# Stand-in for ffmpeg in the transcode manager's tests (like ffmpeg run with -nostdin: ignores stdin, exits on SIGTERM).
# Logs its arguments to calls.log two levels above the session folder (the
# test's root), writes segment <start_number>.ts and the playlist.
for a; do last=$a; done
dir=$(dirname "$last")
echo "$*" >>"$dir/../../calls.log"
n=0
prev=
for a; do
	[ "$prev" = -start_number ] && n=$a
	prev=$a
done
: >"$dir/$n.ts"
printf '#EXTM3U\n#EXTINF:6,\n%s.ts\n#EXT-X-ENDLIST\n' "$n" >"$last"
trap 'exit 255' TERM
sleep 30 &
wait $!
