import sqlite3,zipfile,re,html.parser,urllib.parse,unicodedata,os,sys
LIB=sys.argv[1]
k=sqlite3.connect('latest.sqlite'); l=sqlite3.connect('library.sqlite')
norm=lambda s:re.sub(r'[^a-z0-9]','',unicodedata.normalize('NFKD',s or '').lower())
lib={norm(t):(i,m) for i,t,m in l.execute('select id,title,modified_at from books')}
class T(html.parser.HTMLParser):
    def __init__(s,target=None):
        super().__init__(convert_charrefs=True); s.buf=[]; s.n=0; s.target=target; s.at=None; s.skip=0; s.body=False
    def handle_starttag(s,t,a):
        if t=='body': s.body=True
        if t in('script','style','title'): s.skip+=1
        if dict(a).get('id')==s.target and s.at is None: s.at=s.n
    def handle_endtag(s,t):
        if t in('script','style','title'): s.skip=max(0,s.skip-1)
    def handle_data(s,d):
        if s.body and not s.skip:
            d=re.sub(r'\s','',d); s.buf.append(d); s.n+=len(d)
    def text(s): return ''.join(s.buf)
def parse(z,e,t=None):
    p=T(t); p.feed(z.read(e).decode('utf8','replace')); return p
exact=tot=0; dates={'hit':[],'miss':[]}
for title,ch,cdate in k.execute("select Title,ChapterIDBookmarked,DateCreated from content where ContentType=6 and ContentID like 'file:%' and ChapterIDBookmarked like '%#kobo.%'"):
    bid=lib[norm(title)][0]
    zk=zipfile.ZipFile(f'kep/{bid}.kepub.epub'); zp=zipfile.ZipFile(f'{LIB}/{bid}/book.epub')
    e,frag=urllib.parse.unquote(ch).split('#')
    hit=False
    if e in zk.namelist():
        t=parse(zk,e,frag)
        if t.at is not None and e in zp.namelist():
            hit=True; tot+=1
            a=t.text()[t.at:t.at+40]; b=parse(zp,e).text()[t.at:t.at+40]
            exact+= a==b
    dates['hit' if hit else 'miss'].append((cdate or '')[:10])
print('exact text at mapped offset',exact,'of',tot)
for k_,v in dates.items(): print(k_,len(v),sorted(v)[:3],'..',sorted(v)[-3:])
print('first Parallax commit:')
