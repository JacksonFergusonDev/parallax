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
    pageList?: TocItem[];
    sections: { id: string; linear?: string | null; size?: number }[];
  }
}

declare module "foliate-js/view.js" {
  import type { Book, TocItem } from "foliate-js/epub.js";

  export interface Location {
    cfi: string;
    fraction: number;
    range?: Range;
    section?: { current: number; total: number };
    location?: { current: number; next: number; total: number };
    /** Minutes, roughly, from the size of the text. */
    time?: { section: number; total: number };
    tocItem?: TocItem | null;
    pageItem?: TocItem | null;
  }
  export interface Renderer extends HTMLElement {
    setStyles?(styles: string): void;
    getContents?(): { doc: Document; index: number }[];
    primaryIndex: number;
    nextSection(): Promise<void>;
    scrollToAnchor(anchor: Range, select?: boolean): Promise<void>;
  }
  export interface History extends EventTarget {
    back(): void;
    forward(): void;
    readonly canGoBack: boolean;
    readonly canGoForward: boolean;
  }
  /** What `addAnnotation` draws; `value` is the highlight's CFI. */
  export interface Annotation {
    value: string;
    [key: string]: unknown;
  }
  export interface DrawAnnotation {
    draw(func: unknown, options?: Record<string, unknown>): void;
    annotation: Annotation;
    doc: Document;
    range: Range;
  }
  export interface ShowAnnotation {
    value: string;
    index: number;
    range: Range;
  }
  export interface LinkEvent {
    a: HTMLAnchorElement;
    href: string;
  }
  export interface TTS {
    doc: Document;
    start(): string | undefined;
    resume(): string | undefined;
    next(paused?: boolean): string | undefined;
    from(range: Range): string | undefined;
    setMark(mark: string): Range | undefined;
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
    history: History;
    tts?: TTS;
    initTTS(
      granularity?: "word" | "sentence",
      nodeFilter?: unknown,
      highlighter?: (range: Range) => void,
    ): Promise<void>;
    addAnnotation(annotation: Annotation): Promise<unknown>;
    deleteAnnotation(annotation: Annotation): Promise<unknown>;
    getCFI(index: number, range?: Range): string;
    resolveCFI(cfi: string): { index: number };
    resolveNavigation(
      target: string | number,
    ): { index: number } | null | undefined;
    getSectionFractions(): number[];
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

declare module "foliate-js/overlayer.js" {
  export class Overlayer {
    static highlight: unknown;
    static underline: unknown;
  }
}

declare module "foliate-js/footnotes.js" {
  import type { Book } from "foliate-js/epub.js";
  import type { View } from "foliate-js/view.js";

  export interface FootnoteRender {
    view: View;
    href: string;
    type: string | null;
    hidden: boolean;
  }
  export class FootnoteHandler extends EventTarget {
    detectFootnotes: boolean;
    handle(book: Book, event: Event): Promise<void> | undefined;
  }
}
