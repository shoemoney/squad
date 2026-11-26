#!/usr/bin/env python3
"""Encode a labelled live walkthrough plus its offline export continuation.

Usage: edit_followup.py survey|probe [scratch-directory]
Raw MP4s remain unchanged. FFmpeg runs with two filter/codec threads.
"""
import json
from pathlib import Path
import subprocess
import sys

mode = sys.argv[1]
assert mode in {"survey", "probe"}
scratch = Path(sys.argv[2] if len(sys.argv) > 2 else "/tmp/squad-demo")
main = scratch / f"{mode}.mp4"
report = scratch / f"{mode}-report.mp4"

def duration(path):
    return float(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_entries", "format=duration",
        "-of", "default=nw=1:nk=1", str(path),
    ]))

end = duration(main)
report_end = duration(report)
wait_start, wait_end, speed = 35, end - 14, 20
font = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"

def label(text):
    return ("drawbox=x=0:y=ih-42:w=iw:h=42:color=black@0.85:t=fill,"
            f"drawtext=fontfile={font}:text='{text}':fontcolor=0xffd23f:fontsize=23:x=(w-text_w)/2:y=h-32")

filters = [
    f"[0:v]trim=0:{wait_start},setpts=PTS-STARTPTS,fps=8,scale=1120:-1[v0]",
    f"[0:v]trim={wait_start}:{wait_end},setpts=(PTS-STARTPTS)/{speed},"
    + label(f"TIME CUT - {wait_end-wait_start:.1f} s model wait sped up x{speed}")
    + ",fps=8,scale=1120:-1[v1]",
    f"[0:v]trim={wait_end}:{end},setpts=PTS-STARTPTS,fps=8,scale=1120:-1[v2]",
    "[1:v]setpts=PTS-STARTPTS,"
    + label("OFFLINE EXPORT - same run - no new model calls")
    + ",fps=8,scale=1120:-1[v3]",
    "[v0][v1][v2][v3]concat=n=4:v=1:a=0,split[a][b]",
    "[a]palettegen=max_colors=128:stats_mode=diff[p]",
    "[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
]
temporary = scratch / f"{mode}-followup.gif"
out = Path("docs/media") / f"{mode}.gif"
subprocess.run([
    "ffmpeg", "-v", "error", "-y", "-threads", "2", "-i", str(main),
    "-threads", "2", "-i", str(report), "-filter_complex_threads", "2",
    "-filter_complex", ";".join(filters), str(temporary),
], check=True)
subprocess.run(["gifsicle", "-O3", "--lossy=60", "--colors", "128", str(temporary), "-o", str(out)], check=True)
print(json.dumps({"mode": mode, "raw_seconds": end, "report_seconds": report_end,
    "wait_start": wait_start, "wait_end": wait_end, "speed": speed,
    "gif_seconds": wait_start + (wait_end-wait_start)/speed + 14 + report_end,
    "gif_bytes": out.stat().st_size}))
