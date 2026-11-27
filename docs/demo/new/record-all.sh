#!/usr/bin/env bash
# Record the eleven v0.2.0 mode tapes, cut the README GIFs and scan them for private strings.
#
# usage: OPENROUTER_API_KEY=... docs/demo/new/record-all.sh /path/to/squad [mode ...]
#
# For each mode this writes
#   docs/media/full/<mode>.gif and .mp4   the raw, full-length recording (VHS)
#   docs/media/<mode>.gif                 the README cut: <= 25 s, <= 1.5 MB, opens on activity,
#                                         every sped-up stretch carries a "TIME CUT" label
# Each mode runs with its own temp HOME, SQUAD_DATA_DIR and working directory under /dev/shm, so no
# home or /tmp path can appear on screen. Afterwards every GIF is OCR-scanned; a hit fails the run.
#
# SKIP_RECORD=1 re-cuts and re-scans existing raw recordings without calling any model.
# Cut points default to START/HEAD/TAIL below. Override one mode with CUT_<mode>="a:b:speed ..."
# (raw seconds, speed 1 = real time), e.g. CUT_debug="8:20:1 20:120:10 120:150:1".
# Spend note: this calls real models on the default six-model roster. Run it once, on purpose.
set -euo pipefail

MODES=(forge decide blow_it_up readback forecast tune bass_ackwards_prd rank negotiate debug trace)
MAX_SECONDS=25
MAX_BYTES=1500000
FORBIDDEN='/home/|/Users/|/tmp/|sk-or-'

# Seconds of raw start-up to skip, real-time head after it, real-time tail. The start-up checklist is
# skipped so the clip opens on the prompt being sent (populated first).
START=3
HEAD=8
TAIL=9

usage() {
  echo "usage: OPENROUTER_API_KEY=... $0 /path/to/squad [mode ...]" >&2
  exit 2
}

[[ $# -ge 1 ]] || usage
squad_bin=$(readlink -f "$1")
shift
[[ -x $squad_bin ]] || { echo "not an executable: $squad_bin" >&2; exit 2; }
[[ -n ${OPENROUTER_API_KEY:-} ]] || { echo "OPENROUTER_API_KEY is not set" >&2; exit 2; }
if [[ $# -gt 0 ]]; then MODES=("$@"); fi

for tool in vhs ffmpeg ffprobe gifsicle tesseract tmux; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 2; }
done

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$repo"
mkdir -p docs/media/full

# VHS keeps its frames in TMPDIR; a small /tmp filled up on earlier recordings, so use a disk-backed one.
work_root=$(mktemp -d -p /dev/shm squad-rec.XXXXXX)
vhs_tmp=${VHS_TMPDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/squad-vhs}
mkdir -p "$vhs_tmp"
cleanup() { rm -rf "$work_root"; }
trap cleanup EXIT

mkdir -p "$work_root/bin"
ln -s "$squad_bin" "$work_root/bin/squad"

duration() { ffprobe -v error -show_entries format=duration -of csv=p=0 "$1"; }

record() {
  local mode=$1 dir=$work_root/$1
  mkdir -p "$dir/home" "$dir/data" "$dir/work"
  echo "== recording $mode"
  DEMO_HOME=$dir/home DEMO_WORK=$dir/work SQUAD_DATA_DIR=$dir/data \
    PATH="$work_root/bin:$PATH" TMPDIR=$vhs_tmp \
    vhs "docs/demo/new/$mode.tape"
}

# Default cut: START..START+HEAD real time, the long wait sped up, last TAIL seconds real time.
default_segments() {
  local total=$1 head_end=$((START + HEAD)) tail_start
  tail_start=$(awk -v t="$total" -v tail="$TAIL" 'BEGIN { printf "%.2f", t - tail }')
  awk -v s="$START" -v he="$head_end" -v ts="$tail_start" -v t="$total" -v max="$MAX_SECONDS" -v head="$HEAD" -v tail="$TAIL" '
    BEGIN {
      budget = max - head - tail - 1
      mid = ts - he
      if (mid <= budget) { printf "%d:%.2f:1", s, t; exit }
      speed = int(mid / budget) + 1
      printf "%d:%d:1 %d:%.2f:%d %.2f:%.2f:1", s, he, he, ts, speed, ts, t
    }'
}

# Build the filter graph (same look as docs/demo/tools/edit_live.py) and encode one GIF.
encode() {
  local src=$1 out=$2 width=$3 fps=$4 lossy=$5; shift 5
  local filters="" labels="" i=0 seg a b sp real label
  for seg in "$@"; do
    IFS=: read -r a b sp <<<"$seg"
    filters+="[0:v]trim=$a:$b,setpts=(PTS-STARTPTS)/$sp"
    if awk -v s="$sp" 'BEGIN { exit !(s > 1) }'; then
      real=$(awk -v a="$a" -v b="$b" 'BEGIN { printf "%d", b - a }')
      label="TIME CUT - $real s of model wait sped up x$sp"
      filters+=",drawbox=x=0:y=ih-44:w=iw:h=44:color=black@0.75:t=fill"
      filters+=",drawtext=fontfile=/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf:text='$label':fontcolor=0xffd23f:fontsize=30:x=(w-text_w)/2:y=h-36"
    fi
    filters+=",fps=$fps,scale=$width:-1:flags=lanczos[v$i];"
    labels+="[v$i]"
    i=$((i + 1))
  done
  filters+="${labels}concat=n=$i:v=1:a=0,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle"
  ffmpeg -loglevel error -y -i "$src" -filter_complex "$filters" "$work_root/cut.gif"
  gifsicle -O3 --lossy="$lossy" --colors 128 "$work_root/cut.gif" -o "$out" 2>/dev/null
}

cut() {
  local mode=$1 src=docs/media/full/$1.mp4 out=docs/media/$1.gif
  local total override="CUT_$mode" segs width fps lossy size
  total=$(duration "$src")
  if [[ -n ${!override:-} ]]; then segs=${!override}; else segs=$(default_segments "$total"); fi
  echo "== cutting $mode (raw ${total}s) segments: $segs"
  # Shrink width, frame rate and add loss until the GIF fits the budget.
  for width in 1000 900 800 700; do
    for fps in 10 8; do
      for lossy in 60 90 120; do
        # shellcheck disable=SC2086
        encode "$src" "$out" "$width" "$fps" "$lossy" $segs
        size=$(stat -c %s "$out")
        if (( size <= MAX_BYTES )); then
          echo "   $out: $size bytes at ${width}px ${fps}fps lossy=$lossy"
          return 0
        fi
      done
    done
  done
  echo "FAIL: $out is $size bytes, over $MAX_BYTES" >&2
  return 1
}

check_length() {
  local gif=$1 frames_len
  frames_len=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$gif")
  if awk -v d="$frames_len" -v m="$MAX_SECONDS" 'BEGIN { exit !(d > m + 0.5) }'; then
    echo "FAIL: $gif runs ${frames_len}s, over ${MAX_SECONDS}s" >&2
    return 1
  fi
}

# OCR a frame every few seconds and fail on any private string.
scan() {
  local gif=$1 step=$2 frames
  frames=$(mktemp -d -p "$work_root" scan.XXXXXX)
  ffmpeg -loglevel error -y -i "$gif" -vf "fps=1/$step,scale=1400:-1" "$frames/f%03d.png"
  local f text hit=0
  for f in "$frames"/f*.png; do
    text=$(tesseract "$f" - 2>/dev/null || true)
    if grep -Eq "$FORBIDDEN" <<<"$text"; then
      echo "FAIL: private string in $gif ($(basename "$f")):" >&2
      grep -E "$FORBIDDEN" <<<"$text" >&2
      hit=1
    fi
  done
  return "$hit"
}

failed=0
for mode in "${MODES[@]}"; do
  [[ -n ${SKIP_RECORD:-} ]] || record "$mode"
  cut "$mode" || failed=1
  check_length "docs/media/$mode.gif" || failed=1
  scan "docs/media/$mode.gif" 2 || failed=1
  scan "docs/media/full/$mode.gif" 10 || failed=1
done

if (( failed )); then
  echo "one or more checks failed; do not commit these GIFs" >&2
  exit 1
fi
echo "all ${#MODES[@]} modes recorded, cut and clean"
