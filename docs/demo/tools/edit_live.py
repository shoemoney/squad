#!/usr/bin/env python3
"""edit.py name width fps seg... ; seg = start:end:speed  (speed 1 = real time). Builds a labelled, sped-up GIF."""
import subprocess,sys,os
name,width,fps=sys.argv[1],int(sys.argv[2]),int(sys.argv[3])
segs=[tuple(float(x) for x in s.split(':')) for s in sys.argv[4:]]
src=f'/tmp/squad-demo/{name}.mp4'
parts=[];lab=[]
for i,(a,b,sp) in enumerate(segs):
    f=f"[0:v]trim={a}:{b},setpts=(PTS-STARTPTS)/{sp}"
    if sp>1:
        real=b-a
        txt=f"TIME CUT - {int(real)} s of model wait sped up x{int(sp)}"
        f+=(",drawbox=x=0:y=ih-44:w=iw:h=44:color=black@0.75:t=fill,"
            f"drawtext=fontfile=/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf:text='{txt}':fontcolor=0xffd23f:fontsize=30:x=(w-text_w)/2:y=h-36")
        lab.append((a,b,sp))
    f+=f",fps={fps},scale={width}:-1:flags=lanczos[v{i}]"
    parts.append(f)
fc=';'.join(parts)+';'+''.join(f'[v{i}]' for i in range(len(segs)))+f'concat=n={len(segs)}:v=1:a=0,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle'
out=f'/tmp/sqrec/gif/{name}.gif'
subprocess.run(['ffmpeg','-loglevel','error','-y','-i',src,'-filter_complex',fc,'/tmp/sqrec/tmp2.gif'],check=True)
subprocess.run(['gifsicle','-O3','--lossy=60','--colors','128','/tmp/sqrec/tmp2.gif','-o',out],stderr=subprocess.DEVNULL)
tot=sum((b-a)/s for a,b,s in segs)
print(name,os.path.getsize(out),'bytes',round(tot,1),'s cuts:',[(int(a),int(b),int(s)) for a,b,s in lab])
