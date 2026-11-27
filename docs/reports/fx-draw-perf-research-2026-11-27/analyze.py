import csv,sys,json,math,statistics
path=sys.argv[1]
rows=list(csv.DictReader(open(path)))
cols=list(rows[0])[2:]
for r in rows:
 for c in cols:r[c]=int(r[c])/1e6

def pct(v,p):return sorted(v)[math.ceil(len(v)*p)-1]
summary={}
for tag,rr in [('all',rows),('steady',[r for r in rows if r['resize']<1]),('slow5',[r for r in rows if r['draw']>=pct([r['draw'] for r in rows],.95)])]:
 summary[tag]={'frames':len(rr),'stages':{c:{'mean_ms':statistics.mean(r[c] for r in rr),'p95_ms':pct([r[c] for r in rr],.95),'max_ms':max(r[c] for r in rr),'draw_pct':sum(r[c] for r in rr)/sum(r['draw'] for r in rr)*100} for c in cols}}
for tag,v in summary.items():
 print(tag,v['frames'])
 for c,d in v['stages'].items():print(f" {c:25} mean={d['mean_ms']:.4f} p95={d['p95_ms']:.4f} max={d['max_ms']:.3f} draw%={d['draw_pct']:.1f}")
json.dump(summary,open(path+'.summary.json','w'),indent=2)
