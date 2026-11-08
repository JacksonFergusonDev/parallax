// The parts of the vendored engine the reader uses.
declare module "foliate-js/epub.js" {
  export interface BookLoader {
    entries: { filename: string }[];
    getSize(name: string): number;
    loadText(name: string): Promise<string | null>;
    loadBlob(name: string, type?: string): Promise<Blob | null>;
  }
  export class EPUB {
    constructor(loader: BookLoader);
    init(): Promise<Book>;
  }
  export interface TocItem {
    label: string;
    href: string;
    subitems?: TocItem[] | null;
  }
  export interface Book {
    toc?: TocItem[];
    sections: { id: string; linear?: string | null }[];
  }
}

declare module "foliate-js/view.js" {
  import type { Book, TocItem } from "foliate-js/epub.js";

  export interface Location {
    cfi: string;
    fraction: number;
    range?: Range;
    section?: { current: number; total: number };
    tocItem?: TocItem | null;
  }
  export interface Renderer extends HTMLElement {
    setStyles?(styles: string): void;
    getContents?(): { doc: Document; index: number }[];
  }
  export class View extends HTMLElement {
    book: Book;
    renderer: Renderer;
    lastLocation?: Location;
    open(book: Book): Promise<void>;
    close(): void;
    init(options: {
      lastLocation?: string;
      showTextStart?: boolean;
    }): Promise<void>;
    goTo(target: string | number): Promise<unknown>;
    goToFraction(fraction: number): Promise<void>;
    prev(): Promise<void>;
    next(): Promise<void>;
    goLeft(): Promise<void>;
    goRight(): Promise<void>;
    search(options: SearchOptions): AsyncGenerator<SearchResult | "done">;
    clearSearch(): void;
  }
  export interface SearchOptions {
    query: string;
    matchCase?: boolean;
    matchWholeWords?: boolean;
    matchDiacritics?: boolean;
  }
  export interface SearchMatch {
    cfi: string;
    excerpt: { pre: string; match: string; post: string };
  }
  export type SearchResult =
    | { progress: number }
    | { index: number; label: string; subitems: SearchMatch[] };
}
