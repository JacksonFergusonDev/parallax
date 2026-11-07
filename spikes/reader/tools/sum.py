import json,re,sys,statistics as st
B=[];fail=[];to=[]
for l in open(sys.argv[1]):
    m=re.search(r'\] BOOK (\{.*)',l)
    if m:B.append(json.loads(m[1]))
    if 'OPENFAIL' in l: fail.append(l[:200])
    if 'BOOK-TIMEOUT' in l: to.append(l.strip()[:60])
print('books',len(B),'openfail',len(fail),'timeouts',to)
for f in fail[:5]:print(f)
print('sections',sum(b['sections'] for b in B),'walked',sum(b['walked'] for b in B))
print('blank',[(b['id'],b['blank'][:3]) for b in B if b['blank']][:10])
print('goto timeouts',[(b['id'],b['timeouts'][:4]) for b in B if b['timeouts']][:10])
print('imgs',sum(b['imgs'] for b in B),'broken',sum(b['brokenImgs'] for b in B),[(b['id'],b['brokenImgs']) for b in B if b['brokenImgs']][:12])
fs={};
for b in B:
    for k,v in b['fonts'].items():fs[k]=fs.get(k,0)+v
print('fonts',fs,'books with error fonts',[b['id'] for b in B if b['fonts'].get('error')][:10])
cs={}
for b in B:
    for k,v in b['cspv'].items():cs[k]=cs.get(k,0)+v
print('cspv',cs,[ (b['id'],b['cspv']) for b in B if b['cspv']][:8])
print('cfi ok',sum(b['cfiOk'] for b in B),'bad',sum(b['cfiBad'] for b in B),[(b['id'],b.get('cfiDetail')) for b in B if b['cfiBad']][:8])
t=sorted(x for b in B for x in b['turnMs']);print('turn p50',t[len(t)//2],'p95',t[int(len(t)*.95)],'max',t[-1],'n',len(t),'>=900:',sum(x>=900 for x in t))
print('section load max',sorted(b.get('maxMs',0) for b in B)[-5:]);print('turnErr',[(b['id'],b['turnErr']) for b in B if 'turnErr' in b][:6]); print('fxl',sum(b['fxl'] for b in B))
print('skipped',sum(b['sections']-b['walked'] for b in B))
