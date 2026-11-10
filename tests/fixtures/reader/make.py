#!/usr/bin/env python3
"""Builds the books scripts/reader-e2e.mjs reads.

hostile.epub tries every way found in the reader spike to run a script, load
something from the network, or leave its frame (engineering/decisions/epub-reader.md). Each
script that runs saves reader settings of 200% and dark, which a reader window
is allowed to do, so the test sees it; tries commands a reader isn't granted;
and requests BEACON/exec/<name>. Anything that loads requests BEACON/net/...

long.epub is plain text long enough to turn pages and reopen in the middle.
"""
import html
import zipfile
from pathlib import Path

HERE = Path(__file__).parent
BEACON = "http://127.0.0.1:8931"
SVG = 'xmlns="http://www.w3.org/2000/svg"'


def payload(name):
    reader = "(parent.__TAURI_INTERNALS__||parent.parent.__TAURI_INTERNALS__)"
    return (
        f"new Image().src='{BEACON}/exec/{name}';"
        f"try{{var I={reader};"
        "I.invoke('reader_save_settings',{settings:{fontSize:200,theme:'dark'}});"
        "I.invoke('library_remove',{ids:[1]});"
        "I.invoke('reader_open',{id:1})}catch(e){}"
    )


def attr(name):
    return html.escape(payload(name), quote=True)


def document(body, head=""):
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>'
        f"{head}</head><body>{body}</body></html>"
    )


SCRIPTS = f"""<p>Scripts</p>
<script>{payload("inline")}</script>
<script type="text/javascript" src="evil.js"></script>
<img src="data:," onerror="{attr("onerror")}"/>
<svg {SVG} onload="{attr("svgonload")}" width="1" height="1"></svg>
<svg {SVG} width="1" height="1"><script>{payload("svgscript")}</script></svg>
<svg {SVG} width="1" height="1"><rect width="1" height="1"><animate attributeName="x" from="0" to="1" dur="0.1s" onbegin="{attr("svganimate")}"/></rect></svg>
<iframe srcdoc="{html.escape("<script>" + payload("srcdoc") + "</script>", quote=True)}"></iframe>
<iframe src="data:text/html,{html.escape("<script>" + payload("dataframe") + "</script>", quote=True)}"></iframe>
<iframe src="javascript:{attr("jsframe")}"></iframe>
<iframe src="inner.xhtml"></iframe>
<object data="inner.xhtml" type="application/xhtml+xml"></object>
<embed src="inner.xhtml" type="text/html"/>
<details open="open" ontoggle="{attr("ontoggle")}"><summary>s</summary></details>
<input autofocus="autofocus" onfocus="{attr("onfocus")}"/>
<style onload="{attr("styleonload")}">p{{}}</style>
<video src="missing.mp4" onerror="{attr("videoerror")}"></video>
"""

REMOTE = f"""<p>Remote</p>
<img src="{BEACON}/net/img"/>
<img src="data:," srcset="{BEACON}/net/srcset 1x"/>
<picture><source srcset="{BEACON}/net/picture"/><img src="data:,"/></picture>
<div style="background:url({BEACON}/net/inlinecss)">x</div>
<audio src="{BEACON}/net/audio" autoplay="autoplay"></audio>
<video src="{BEACON}/net/video" poster="{BEACON}/net/poster"></video>
<svg {SVG} xmlns:xlink="http://www.w3.org/1999/xlink" width="1" height="1"><image href="{BEACON}/net/svgimage" width="1" height="1"/><image xlink:href="{BEACON}/net/svgxlink" width="1" height="1"/></svg>
<object data="{BEACON}/net/object"></object><embed src="{BEACON}/net/embed"/>
<iframe src="{BEACON}/net/iframe"></iframe>
<form action="{BEACON}/net/form"><input type="image" src="{BEACON}/net/inputimage"/></form>
"""

REMOTE_HEAD = (
    f'<link rel="stylesheet" href="{BEACON}/net/linkcss"/>'
    '<link rel="stylesheet" href="book.css"/>'
    f'<link rel="preload" as="image" href="{BEACON}/net/preload"/>'
    f"<style>@import url({BEACON}/net/importcss);"
    f"@font-face{{font-family:z;src:url({BEACON}/net/font)}}"
    f"body{{font-family:z;background:url({BEACON}/net/stylecss)}}</style>"
)

LEAVING_HEAD = (
    f'<meta http-equiv="refresh" content="0;url={BEACON}/net/refresh"/>'
    f'<base href="{BEACON}/net/base/"/>'
    '<meta http-equiv="Content-Security-Policy" '
    "content=\"default-src *; script-src * 'unsafe-inline'\"/>"
)


def write_epub(path, title, chapters, extra=(), scripted=False):
    manifest = "".join(
        f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"'
        f'{" properties=\"scripted\"" if scripted else ""}/>'
        for i in range(len(chapters))
    )
    for name, media_type, _ in extra:
        manifest += f'<item id="{name.replace(".", "-")}" href="{name}" media-type="{media_type}"/>'
    spine = "".join(f'<itemref idref="c{i}"/>' for i in range(len(chapters)))
    toc = "".join(
        f'<li><a href="c{i}.xhtml">Chapter {i + 1}</a></li>' for i in range(len(chapters))
    )
    files = {
        "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" '
        'xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles>'
        '<rootfile full-path="book.opf" media-type="application/oebps-package+xml"/>'
        "</rootfiles></container>",
        "book.opf": '<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" '
        'version="3.0" unique-identifier="id"><metadata '
        'xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">'
        f"{path.stem}</dc:identifier><dc:title>{title}</dc:title>"
        "<dc:creator>Test Fixture</dc:creator><dc:language>en</dc:language>"
        '<meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata>'
        '<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" '
        f'properties="nav"/>{manifest}</manifest><spine>{spine}</spine></package>',
        "nav.xhtml": document(
            '<nav xmlns:epub="http://www.idpf.org/2007/ops" epub:type="toc">'
            f"<ol>{toc}</ol></nav>"
        ),
    }
    for i, chapter in enumerate(chapters):
        files[f"c{i}.xhtml"] = chapter
    for name, _, data in extra:
        files[name] = data
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        info = zipfile.ZipInfo("mimetype", date_time=(2026, 1, 1, 0, 0, 0))
        z.writestr(info, "application/epub+zip", zipfile.ZIP_STORED)
        for name, data in files.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data)


def main():
    inner = document(f"<script>{payload('innerdoc').replace('parent.', 'parent.parent.')}</script>")
    css = (
        f"@import url({BEACON}/net/bookimport);"
        f"p{{background:url({BEACON}/net/bookcssbg)}}"
        f"@font-face{{font-family:y;src:url({BEACON}/net/bookfont)}}"
    )
    write_epub(
        HERE / "hostile.epub",
        "Hostile",
        [
            document(SCRIPTS),
            document(REMOTE, REMOTE_HEAD),
            document('<p>Leaving</p><img src="relative.png"/>', LEAVING_HEAD),
            document(f"<p>Last</p><script>{payload('last')}</script>"),
        ],
        extra=[
            ("inner.xhtml", "application/xhtml+xml", inner),
            ("evil.js", "text/javascript", payload("external")),
            ("book.css", "text/css", css),
        ],
        scripted=True,
    )
    words = (
        "Every chapter in this book repeats a different sentence so that each "
        "page has its own text, and the reader can be reopened partway through."
    )
    write_epub(
        HERE / "long.epub",
        "Long",
        [
            document(
                f"<h1>Chapter {c + 1}</h1>"
                + "".join(f"<p>Paragraph {c + 1}.{p + 1}. {words}</p>" for p in range(80))
            )
            for c in range(6)
        ],
    )


if __name__ == "__main__":
    main()
