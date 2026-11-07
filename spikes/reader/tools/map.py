import sqlite3,zipfile,re,os,subprocess,sys,html.parser,xml.etree.ElementTree as ET,posixpath,json,urllib.parse,unicodedata
KEP=sys.argv[1]; LIB=sys.argv[2]
k=sqlite3.connect('latest.sqlite'); l=sqlite3.connect('library.sqlite')
norm=lambda s:re.sub(r'[^a-z0-9]','',unicodedata.normalize('NFKD',s or '').lower())
cols=[r[1] for r in l.execute('pragma table_info(books)')]
lib={norm(t):i for i,t in l.execute('select id,title from books')}
rows=k.execute("select Title,ChapterIDBookmarked,___PercentRead,ReadStatus,ContentID from content where ContentType=6 and ContentID like 'file:%' and ChapterIDBookmarked like '%#kobo.%'").fetchall()
class T(html.parser.HTMLParser):
    def __init__(s,target=None):
        super().__init__(convert_charrefs=True); s.n=0; s.target=target; s.at=None; s.skip=0; s.ids=0; s.body=False
    def handle_starttag(s,t,a):
        if t=='body': s.body=True
        if t in('script','style','title'): s.skip+=1
        d=dict(a)
        if t=='span' and (d.get('id') or '').startswith('kobo.'): s.ids+=1
        if d.get('id')==s.target and s.at is None: s.at=s.n
    def handle_startendtag(s,t,a):
        d=dict(a)
        if d.get('id')==s.target and s.at is None: s.at=s.n
    def handle_endtag(s,t):
        if t in('script','style','title'): s.skip=max(0,s.skip-1)
    def handle_data(s,d):
        if s.body and not s.skip: s.n+=len(re.sub(r'\s','',d))
def txt(z,name,target=None):
    p=T(target); p.feed(z.read(name).decode('utf-8','replace')); return p
def spine(z):
    c=ET.fromstring(z.read('META-INF/container.xml')); opf=c.find('.//{*}rootfile').get('full-path')
    o=ET.fromstring(z.read(opf)); base=posixpath.dirname(opf)
    m={i.get('id'):i.get('href') for i in o.find('{*}manifest')}
    return [posixpath.normpath(posixpath.join(base,urllib.parse.unquote(m[r.get('idref')]))) for r in o.find('{*}spine') if r.get('idref') in m]
st=dict(rows=len(rows),matched=0,entry=0,span=0,sametext=0,plain_entry=0); out=[]
for title,ch,pct,rs,cid in rows:
    bid=lib.get(norm(title))
    if bid is None: out.append(('NOLIB',title[:30])); continue
    p=f'{LIB}/{bid}/book.epub'
    if not os.path.exists(p): continue
    st['matched']+=1
    kp=f'kep/{bid}.kepub.epub'
    os.makedirs('kep',exist_ok=True)
    if not os.path.exists(kp): subprocess.run([KEP,'-o',kp,p],capture_output=True)
    if not os.path.exists(kp): out.append(('NOKEP',bid)); continue
    zk=zipfile.ZipFile(kp); zp=zipfile.ZipFile(p)
    entry,frag=urllib.parse.unquote(ch).split('#')[0],ch.split('#')[1]
    names=set(zk.namelist())
    if entry not in names: out.append(('NOENTRY',bid,entry[-40:])); continue
    st['entry']+=1
    t=txt(zk,entry,frag)
    if t.at is None: out.append(('NOSPAN',bid,frag,t.ids)); continue
    st['span']+=1
    if entry in zp.namelist():
        st['plain_entry']+=1
        tp=txt(zp,entry)
        same=tp.n==t.n
        st['sametext']+=same
    else: same=None
    sp=[e for e in spine(zp) if e in zp.namelist()]
    sizes=[txt(zp,e).n for e in sp]
    tot=sum(sizes) or 1
    if entry in sp:
        i=sp.index(entry); calc=(sum(sizes[:i])+t.at)/tot*100
    else: calc=None
    out.append(('OK',bid,frag,'pct',pct,'calc',None if calc is None else round(calc,1),'same',same,'off',t.at,'of',t.n))
print(st)
d=[abs(o[4]-o[6]) for o in out if o[0]=='OK' and o[6] is not None]
print('pct diffs',sorted(round(x,1) for x in d))
for o in out:
    if o[0]!='OK': print(o)
for o in [o for o in out if o[0]=='OK'][:12]: print(o)
