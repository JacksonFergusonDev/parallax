import zipfile,sys
B='http://127.0.0.1:8931'
lorem=' '.join(['The quick reader tests selection, keyboard and wheel behaviour in a sandboxed frame.']*8)
paras=''.join(f'<p>{lorem}</p>' for _ in range(40))
c1=f'''<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>One</title><link rel="stylesheet" href="s.css"/></head><body>
<h1 id="h">Chapter one</h1><p id="sel">Select this sentence for the highlight test because it is long enough to drag across two words at least.</p>
<p>Links: <a id="lnk" href="c2.xhtml#t">internal</a> and <a id="ext" href="{B}/net/extlink">external</a> and <a id="extb" target="_blank" href="{B}/net/extblank">blank</a> and note<a id="nref" epub:type="noteref" href="#fn1">1</a>.</p>
{paras}<aside id="fn1" epub:type="footnote"><p>Footnote body text here.</p></aside></body></html>'''
c2=f'''<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Two</title></head><body><h1 id="t">Chapter two</h1>{paras}</body></html>'''
nav='''<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>n</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml">One</a></li><li><a href="c2.xhtml">Two</a></li></ol></nav></body></html>'''
opf='''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="i">events-1</dc:identifier><dc:title>Events</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="a" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="b" href="c2.xhtml" media-type="application/xhtml+xml"/><item id="s" href="s.css" media-type="text/css"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>'''
with zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED) as z:
    z.writestr(zipfile.ZipInfo('mimetype'),'application/epub+zip',zipfile.ZIP_STORED)
    z.writestr('META-INF/container.xml','<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    for n,v in [('c.opf',opf),('nav.xhtml',nav),('c1.xhtml',c1),('c2.xhtml',c2),('s.css','h1{color:rgb(10,20,30)}')]: z.writestr(n,v)
