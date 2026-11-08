// Read aloud. The engine splits the page into sentences and hands back SSML
// with a <mark> at each; the system voice reads the plain text, and its word
// boundaries tell us which sentence to light up.

export interface Mark {
  offset: number;
  name: string;
}

export interface SpeechText {
  text: string;
  marks: Mark[];
  lang: string | null;
}

const XML_NS = "http://www.w3.org/XML/1998/namespace";

/** The words to say, and where each mark falls in them. */
export function ssmlToText(ssml: string): SpeechText {
  const doc = new DOMParser().parseFromString(ssml, "application/xml");
  if (doc.querySelector("parsererror"))
    return { text: "", marks: [], lang: null };
  let text = "";
  const marks: Mark[] = [];
  const walk = (node: Node) => {
    if (
      node.nodeType === Node.TEXT_NODE ||
      node.nodeType === Node.CDATA_SECTION_NODE
    ) {
      text += node.textContent ?? "";
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const element = node as Element;
    switch (element.localName) {
      case "mark":
        marks.push({
          offset: text.length,
          name: element.getAttribute("name") ?? "",
        });
        return;
      case "break":
        text += " ";
        return;
      case "sub": {
        const alias = element.getAttribute("alias");
        if (alias !== null) {
          text += alias;
          return;
        }
      }
    }
    node.childNodes.forEach(walk);
  };
  walk(doc.documentElement);
  const root = doc.documentElement;
  const lang =
    root.getAttributeNS(XML_NS, "lang") || root.getAttribute("xml:lang");
  return { text, marks, lang: lang || null };
}

/** The last mark at or before `index`. */
export function markAt(marks: Mark[], index: number): string | null {
  let found: string | null = null;
  for (const mark of marks) {
    if (mark.offset > index) break;
    found = mark.name;
  }
  return found;
}

export const RATES = [0.8, 1, 1.25, 1.5, 1.75, 2];

/** The next speed, going round. */
export const nextRate = (rate: number) =>
  RATES[(RATES.indexOf(rate) + 1) % RATES.length] ?? 1;

/** Where the words come from: the engine, crossing into later chapters. */
export interface SpeechSource {
  /** The first sentence to read, from the page on screen. */
  start(): Promise<string | undefined>;
  /** The sentence that was being read, from its last mark. */
  resume(): string | undefined;
  /** The next block, or nothing at the end of the book. */
  next(): Promise<string | undefined>;
  mark(name: string): void;
  /** Reading has stopped; take any highlight off the page. */
  stopped(): void;
}

export interface Voice {
  speak(utterance: SpeechSynthesisUtterance): void;
  cancel(): void;
}

export type SpeakerState = "stopped" | "playing" | "paused";

export class Speaker {
  state: SpeakerState = "stopped";
  rate = 1;
  /** Each utterance belongs to one run; cancelling starts another. */
  #run = 0;

  constructor(
    private source: SpeechSource,
    private onState: (state: SpeakerState) => void,
    private voice: Voice = window.speechSynthesis,
    private utterance = (text: string) => new SpeechSynthesisUtterance(text),
  ) {}

  #set(state: SpeakerState) {
    this.state = state;
    this.onState(state);
  }

  async play() {
    if (this.state === "playing") return;
    const resuming = this.state === "paused";
    this.#set("playing");
    const run = ++this.#run;
    const ssml = resuming ? this.source.resume() : await this.source.start();
    if (run === this.#run) await this.#say(ssml, run);
  }

  pause() {
    if (this.state !== "playing") return;
    this.#run++;
    this.voice.cancel();
    this.#set("paused");
  }

  stop() {
    if (this.state === "stopped") return;
    this.#run++;
    this.voice.cancel();
    this.source.stopped();
    this.#set("stopped");
  }

  /** Changes speed, picking up again from the current sentence. */
  setRate(rate: number) {
    this.rate = rate;
    if (this.state !== "playing") return;
    this.pause();
    void this.play();
  }

  async #say(ssml: string | undefined, run: number) {
    // Blocks with nothing to say, such as an image alone, are skipped.
    let spoken = ssml === undefined ? null : ssmlToText(ssml);
    while (spoken && !spoken.text.trim()) {
      const next = await this.source.next();
      if (run !== this.#run) return;
      spoken = next === undefined ? null : ssmlToText(next);
    }
    if (!spoken) {
      this.stop();
      return;
    }
    const { text, marks, lang } = spoken;
    const utterance = this.utterance(text);
    utterance.rate = this.rate;
    if (lang) utterance.lang = lang;
    if (marks[0]) this.source.mark(marks[0].name);
    let current = marks[0]?.name ?? null;
    utterance.onboundary = ({ charIndex }) => {
      if (run !== this.#run) return;
      const name = markAt(marks, charIndex);
      if (name !== null && name !== current) {
        current = name;
        this.source.mark(name);
      }
    };
    const finished = () => {
      if (run !== this.#run) return;
      void this.source.next().then((next) => {
        if (run === this.#run) void this.#say(next, run);
      });
    };
    utterance.onend = finished;
    utterance.onerror = (event) => {
      // Cancelling reports an error too; only a real one stops reading.
      if (
        run !== this.#run ||
        event.error === "interrupted" ||
        event.error === "canceled"
      )
        return;
      console.error("speech failed", event.error);
      this.stop();
    };
    this.voice.speak(utterance);
  }
}
