#!/bin/bash
# usage: mkgif.sh in.mp4 start dur out.gif [width] [fps] [extra-vf]
in=$1; ss=$2; d=$3; out=$4; w=${5:-960}; fps=${6:-8}; extra=${7:-}
vf="fps=$fps,scale=$w:-1:flags=lanczos${extra:+,$extra}"
ffmpeg -loglevel error -y -ss $ss -t $d -i $in -vf "$vf,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" /tmp/sqrec/tmp.gif
gifsicle -O3 --lossy=60 --colors 128 /tmp/sqrec/tmp.gif -o $out
ls -la $out | awk '{print $5, $9}'
