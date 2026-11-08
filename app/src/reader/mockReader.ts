// Browser-only stand-in for Rust, so the reader can be developed with
// `pnpm dev` at /reader.html. Never part of the app bundle.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

const PARAGRAPH =
  "<p>The quick brown fox jumps over the lazy dog near the riverbank at dusk, while the reader turns another page and wonders how the sentence will end.</p>";

const chapter = (
  title: string,
  page: number,
) => `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>${title}</title></head><body>
<h1>${title}</h1>
<p><span epub:type="pagebreak" id="p${page}"/>A sentence with a note.<a epub:type="noteref" href="notes.xhtml#n${page}">${page}</a> Then the story goes on.</p>
${PARAGRAPH.repeat(30)}<p><span epub:type="pagebreak" id="p${page + 1}"/></p>${PARAGRAPH.repeat(30)}
</body></html>`;

const NOTES = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Notes</title></head><body>
<h1>Notes</h1>
<aside epub:type="footnote" id="n1"><p>The first note, shown over the page.</p></aside>
<aside epub:type="footnote" id="n3"><p>The second note, from chapter two.</p></aside>
</body></html>`;

const FILES: Record<string, string> = {
  "META-INF/container.xml": `<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>`,
  "book.opf": `<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">mock</dc:identifier><dc:title>A Sample Book</dc:title><dc:language>en</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/><itemref idref="notes" linear="no"/></spine></package>`,
  "nav.xhtml": `<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="one.xhtml">Chapter one</a></li><li><a href="two.xhtml">Chapter two</a></li></ol></nav><nav epub:type="page-list"><ol><li><a href="one.xhtml#p1">1</a></li><li><a href="one.xhtml#p2">2</a></li><li><a href="two.xhtml#p3">3</a></li><li><a href="two.xhtml#p4">4</a></li></ol></nav></body></html>`,
  "one.xhtml": chapter("Chapter one", 1),
  "two.xhtml": chapter("Chapter two", 3),
  "notes.xhtml": NOTES,
};

interface MockHighlight {
  id: number;
  created_at: string;
  updated_at: string;
  [key: string]: unknown;
}

export function installMockReader() {
  mockWindows("reader-1");
  let highlights: MockHighlight[] = [];
  let nextId = 1;
  mockIPC((command, args) => {
    const encoder = new TextEncoder();
    switch (command) {
      case "reader_book":
        return {
          title: "A Sample Book",
          entries: Object.entries(FILES).map(([name, text]) => ({
            name,
            size: text.length,
          })),
          position: null,
          settings: { fontSize: 100, theme: "auto" },
          // ?appTheme=sepia previews the reader under another app theme.
          app_theme:
            new URLSearchParams(location.search).get("appTheme") ?? "system",
        };
      case "reader_entry": {
        const { name } = args as { name: string };
        return encoder.encode(FILES[name]).buffer;
      }
      case "reader_highlights":
        return highlights;
      case "reader_save_highlight": {
        const { id, highlight } = args as {
          id: number | null;
          highlight: Record<string, unknown>;
        };
        const now = new Date().toISOString();
        const old = highlights.find((h) => h.id === id);
        const saved: MockHighlight = {
          ...highlight,
          id: old?.id ?? nextId++,
          created_at: old?.created_at ?? now,
          updated_at: now,
        };
        highlights = [...highlights.filter((h) => h.id !== saved.id), saved];
        return saved;
      }
      case "reader_delete_highlight": {
        const { id } = args as { id: number };
        highlights = highlights.filter((h) => h.id !== id);
        return null;
      }
      default:
        return null;
    }
  });
}
