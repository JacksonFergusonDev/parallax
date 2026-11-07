// Browser-only stand-in for Rust, so the reader can be developed with
// `pnpm dev` at /reader.html. Never part of the app bundle.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

const chapter = (title: string) => `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>${title}</title></head><body>
<h1>${title}</h1>${"<p>The quick brown fox jumps over the lazy dog near the riverbank at dusk, while the reader turns another page and wonders how the sentence will end.</p>".repeat(60)}
</body></html>`;

const FILES: Record<string, string> = {
  "META-INF/container.xml": `<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>`,
  "book.opf": `<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">mock</dc:identifier><dc:title>A Sample Book</dc:title><dc:language>en</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/></spine></package>`,
  "nav.xhtml": `<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="one.xhtml">Chapter one</a></li><li><a href="two.xhtml">Chapter two</a></li></ol></nav></body></html>`,
  "one.xhtml": chapter("Chapter one"),
  "two.xhtml": chapter("Chapter two"),
};

export function installMockReader() {
  mockWindows("reader-1");
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
        };
      case "reader_entry": {
        const { name } = args as { name: string };
        return encoder.encode(FILES[name]).buffer;
      }
      default:
        return null;
    }
  });
}
