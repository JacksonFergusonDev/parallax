import zipfile,sys,random
def book(path,name,docs,opfmeta='',spineattr='',lang='en',props='',extra={}):
    man=''.join(f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"{props}/>' for i in range(len(docs)))
    sp=''.join(f'<itemref idref="c{i}"/>' for i in range(len(docs)))
    for n,(mt,_) in extra.items(): man+=f'<item id="x{abs(hash(n))}" href="{n}" media-type="{mt}"/>'
    opf=f'''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="i" prefix="rendition: http://www.idpf.org/vocab/rendition/#"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="i">{name}</dc:identifier><dc:title>{name}</dc:title><dc:language>{lang}</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>{opfmeta}</metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>{man}</manifest><spine {spineattr}>{sp}</spine></package>'''
    nav='<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>n</title></head><body><nav epub:type="toc"><ol>'+''.join(f'<li><a href="c{i}.xhtml">{i}</a></li>' for i in range(len(docs)))+'</ol></nav></body></html>'
    with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as z:
        z.writestr(zipfile.ZipInfo('mimetype'),'application/epub+zip',zipfile.ZIP_STORED)
        z.writestr('META-INF/container.xml','<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        z.writestr('c.opf',opf); z.writestr('nav.xhtml',nav)
        for i,d in enumerate(docs): z.writestr(f'c{i}.xhtml',d)
        for n,(mt,b) in extra.items(): z.writestr(n,b)
H=lambda body,head='',attr='':f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"{attr}><head><title>t</title>{head}</head><body>{body}</body></html>'
d=sys.argv[1]
s='The quick brown fox jumps over the lazy dog near the riverbank at dusk. '
# self-closed tags
book(f'{d}/selfclosed.epub','selfclosed',['<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title/><script src="a.js"/></head><body><a id="pg12"/><p>After anchor text survives.</p><div/><p>After empty div text survives.</p><br/><p>End marker text.</p></body></html>'])
# malformed xhtml
book(f'{d}/malformed.epub','malformed',['<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>x</title></head><body><p>Unclosed para<br>&nbsp; bad entity &foo; still readable text.<p>Second para</body></html>'])
# big single file ~20MB
para=f'<p>{s*12}</p>'
big=H(''.join(f'<h2 id="s{i}">Part {i}</h2>'+para*20 for i in range(1100)))
book(f'{d}/big20mb.epub','big20mb',[big]); print('big bytes',len(big))
# vertical japanese
ja='吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。'
css='<style>html{writing-mode:vertical-rl;-webkit-writing-mode:vertical-rl}</style>'
book(f'{d}/vertical.epub','vertical',[H(f'<h1>第一章</h1>'+f'<p>{ja*6}</p>'*40,css,' lang="ja"'),H(f'<h1>第二章</h1>'+f'<p>{ja*6}</p>'*40,css,' lang="ja"')],spineattr='page-progression-direction="rtl"',lang='ja')
# rtl
ar='هذا نص عربي طويل لاختبار الاتجاه من اليمين إلى اليسار في القارئ. '
book(f'{d}/rtl.epub','rtl',[H('<h1>الفصل الأول</h1>'+f'<p>{ar*12}</p>'*40,'',' lang="ar" dir="rtl"'),H('<h1>الفصل الثاني</h1>'+f'<p>{ar*12}</p>'*40,'',' lang="ar" dir="rtl"')],spineattr='page-progression-direction="rtl"',lang='ar')
# fixed layout
fx=lambda i:f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>p</title><meta name="viewport" content="width=600,height=800"/></head><body style="margin:0;width:600px;height:800px;background:#eef"><div style="position:absolute;top:{50+i*40}px;left:60px;font-size:40px">Page {i}</div></body></html>'
book(f'{d}/fxl.epub','fxl',[fx(i) for i in range(5)],opfmeta='<meta property="rendition:layout">pre-paginated</meta><meta property="rendition:spread">landscape</meta>')
