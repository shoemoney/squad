#!/usr/bin/env python3
"""edit_hero.py RAW.mp4 OUT.gif WIDTH FPS seg... ; seg = start:end:speed (speed 1 = real time).
Builds the labelled, sped-up hero GIF from the uncut VHS recording. Every segment faster than 1x
carries an on-screen label stating the real seconds it covers."""
import os, subprocess, sys, tempfile
src, out, width, fps = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
segs = [tuple(float(x) for x in s.split(':')) for s in sys.argv[5:]]
font = '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'
parts = []
for i, (a, b, sp) in enumerate(segs):
    f = f"[0:v]trim={a}:{b},setpts=(PTS-STARTPTS)/{sp}"
    if sp > 1:
        real = round(b - a)
        txt = (f"TIME CUT - {real} s of model wait sped up x{sp:g}" if sp >= 8
               else f"typing sped up x{sp:g}")
        f += (",drawbox=x=0:y=26:w=iw:h=44:color=black@0.75:t=fill,"
              f"drawtext=fontfile={font}:text='{txt}':fontcolor=0xffd23f:fontsize=28:x=(w-text_w)/2:y=34")
    f += f",fps={fps},scale={width}:-1:flags=lanczos[v{i}]"
    parts.append(f)
fc = ';'.join(parts) + ';' + ''.join(f'[v{i}]' for i in range(len(segs))) + \
     f'concat=n={len(segs)}:v=1:a=0,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];' \
     '[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle'
with tempfile.TemporaryDirectory() as d:
    tmp = os.path.join(d, 'tmp.gif')
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', src, '-filter_complex', fc, tmp], check=True)
    subprocess.run(['gifsicle', '-O3', '--lossy=80', '--colors', '128', tmp, '-o', out], check=True)
print(out, os.path.getsize(out), 'bytes', round(sum((b - a) / s for a, b, s in segs), 1), 's')
