import zipfile,sys
B='http://127.0.0.1:8931'
s=f"""setTimeout(function(){{try{{window.open('{B}/net/winopen')}}catch(e){{}}}},200);setTimeout(function(){{var a=document.createElement('a');a.href='{B}/net/aclick';a.target='_top';document.body.appendChild(a);a.click()}},500);setTimeout(function(){{try{{parent.location.href='{B}/net/parentloc'}}catch(e){{}}}},900);setTimeout(function(){{location.href='{B}/net/selfloc'}},1300);"""
doc=f'''<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>n</title></head><body><p>nav attempts</p><script>{s}</script></body></html>'''
opf='''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="i">n1</dc:identifier><dc:title>Nav</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest><item id="a" href="c1.xhtml" media-type="application/xhtml+xml" properties="scripted"/></manifest><spine><itemref idref="a"/></spine></package>'''
with zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED) as z:
    z.writestr(zipfile.ZipInfo('mimetype'),'application/epub+zip',zipfile.ZIP_STORED)
    z.writestr('META-INF/container.xml','<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    z.writestr('c.opf',opf); z.writestr('c1.xhtml',doc)
