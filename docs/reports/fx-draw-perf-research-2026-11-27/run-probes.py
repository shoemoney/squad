import os,subprocess,pathlib
out=pathlib.Path('docs/reports/fx-draw-perf-research-2026-11-27')
binary=os.environ['BENCH']
probes=[('control2',{}),('direct',{'RESEARCH_DIRECT_STATE':'1'}),('activity',{'RESEARCH_FAST_ACTIVITY':'1'}),('dirty',{'RESEARCH_DIRTY_ONLY':'1'}),('dirty-activity',{'RESEARCH_DIRTY_ONLY':'1','RESEARCH_FAST_ACTIVITY':'1'}),('anchors',{'RESEARCH_SKIP_ANCHORS':'1'}),('history-500',{'RESEARCH_HISTORY':'500','COMPOSED_SECS':'6'}),('history-0',{'RESEARCH_HISTORY':'0','COMPOSED_SECS':'6'})]
for name,extra in probes:
 env={k:v for k,v in os.environ.items() if not k.startswith('RESEARCH_')};env.update(COMPOSED_SECS='30',COMPOSED_VIEWPORT='140x42',COMPOSED_ENFORCE='0',RESEARCH_CSV=str(out/f'{name}.csv'));env.update(extra)
 print('START',name,extra,flush=True)
 with (out/f'timing-{name}.log').open('w') as f:
  p=subprocess.run([binary],env=env,stdout=f,stderr=subprocess.STDOUT)
 print((out/f'timing-{name}.log').read_text(),flush=True)
 if p.returncode:raise SystemExit(p.returncode)
 subprocess.run(['python3',str(out/'analyze.py'),str(out/f'{name}.csv')],stdout=open(f'/tmp/fx-{name}.summary.txt','w'),check=True)
