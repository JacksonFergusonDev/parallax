#!/usr/bin/env python3
"""Builds the Amazon-format test books with calibre's ebook-convert.

The book is synthetic: a cover, an inline image, a stylesheet, a footnote that
links across chapters, and a link to the start of a chapter. calibre gives a
target with no id only an `aid`, which is what the link fix in vendor/boko is
about; the image fix shows on combo.mobi.
"""

import struct
import subprocess
import tempfile
import zipfile
import zlib
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[1] / "tests/fixtures/amazon"


def png(width: int, height: int, rgb: tuple[int, int, int]) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    rows = b"".join(b"\0" + bytes(rgb) * width for _ in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def page(title: str, body: str) -> str:
    return f"""<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title>
<link rel="stylesheet" type="text/css" href="style.css"/></head>
<body>{body}</body></html>"""


ONE = page(
    "Chapter One",
    "<h1>Chapter One</h1>"
    + "<p>Opening words of the test book.</p>" * 40
    + '<p class="fancy">See the note<a href="two.xhtml#note1">1</a>.</p>'
    + '<p><a href="two.xhtml">On to chapter two</a>.</p>'
    + '<p><img src="plate.png" alt="plate"/></p>',
)
TWO = page(
    "Chapter Two",
    "<h1>Chapter Two</h1>"
    + "<p>Filler before the note.</p>" * 40
    + '<p id="note1">1. The footnote text.</p>'
    + "<p>Filler after the note.</p>" * 40,
)
OPF = """<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>Amazon Fixture</dc:title><dc:creator>Test Author</dc:creator>
<dc:language>en</dc:language><dc:identifier id="id">parallax-amazon-fixture</dc:identifier>
<meta name="cover" content="cover"/></metadata>
<manifest>
<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
<item id="css" href="style.css" media-type="text/css"/>
<item id="cover" href="cover.png" media-type="image/png"/>
<item id="plate" href="plate.png" media-type="image/png"/>
<item id="one" href="one.xhtml" media-type="application/xhtml+xml"/>
<item id="two" href="two.xhtml" media-type="application/xhtml+xml"/>
</manifest>
<spine toc="ncx"><itemref idref="one"/><itemref idref="two"/></spine></package>"""
NCX = """<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
<head><meta name="dtb:uid" content="parallax-amazon-fixture"/></head>
<docTitle><text>Amazon Fixture</text></docTitle><navMap>
<navPoint id="n1" playOrder="1"><navLabel><text>Chapter One</text></navLabel><content src="one.xhtml"/></navPoint>
<navPoint id="n2" playOrder="2"><navLabel><text>Chapter Two</text></navLabel><content src="two.xhtml"/></navPoint>
</navMap></ncx>"""
CONTAINER = """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""

with tempfile.TemporaryDirectory() as tmp:
    epub = Path(tmp) / "book.epub"
    with zipfile.ZipFile(epub, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", CONTAINER)
        z.writestr("content.opf", OPF)
        z.writestr("toc.ncx", NCX)
        z.writestr("style.css", ".fancy { color: #336; font-style: italic; }")
        z.writestr("cover.png", png(120, 180, (40, 80, 160)))
        z.writestr("plate.png", png(64, 64, (200, 60, 40)))
        z.writestr("one.xhtml", ONE)
        z.writestr("two.xhtml", TWO)
    for name, args in (
        ("book.azw3", []),
        ("combo.mobi", ["--mobi-file-type", "both"]),
        ("old.mobi", ["--mobi-file-type", "old"]),
    ):
        subprocess.run(
            ["ebook-convert", epub, FIXTURES / name, *args],
            check=True,
            capture_output=True,
        )
