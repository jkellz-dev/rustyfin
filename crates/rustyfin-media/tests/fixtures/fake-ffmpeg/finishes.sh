#!/bin/sh
# Stand-in for ffmpeg in the transcode manager's tests (finishes the file: exit 0).
# Logs its arguments to calls.log two levels above the session folder (the
# test's root), writes the run's first segment (-start_number or
# -segment_start_number, named by -hls_segment_filename or the segment
# muxer's output pattern) and lists it in the run's list (the HLS muxer's
# playlist, the last argument, or the segment muxer's -segment_list).
for a; do last=$a; done
n=0
pattern=
list=
prev=
for a; do
	case $prev in
	-start_number | -segment_start_number) n=$a ;;
	-hls_segment_filename) pattern=$a ;;
	-segment_list) list=$a ;;
	esac
	prev=$a
done
if [ -n "$pattern" ]; then
	list=$last
else
	pattern=$last
fi
dir=$(dirname "$list")
echo "$*" >>"$dir/../../calls.log"
# shellcheck disable=SC2059 # the pattern is ffmpeg's, %d for the number
segment=$(printf "$pattern" "$n")
: >"$segment"
printf '#EXTM3U\n#EXTINF:6,\n%s\n' "$(basename "$segment")" >"$list"
exit 0
