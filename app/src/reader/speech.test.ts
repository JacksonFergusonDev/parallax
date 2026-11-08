// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import {
  Speaker,
  markAt,
  nextRate,
  ssmlToText,
  type SpeechSource,
  type Voice,
} from "./speech";

const SSML = (body: string, lang = "en") =>
  `<speak xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="${lang}">${body}</speak>`;

describe("ssmlToText", () => {
  it("keeps the words and where each mark falls", () => {
    const spoken = ssmlToText(
      SSML(
        '<mark name="0"/>It was <emphasis>dark</emphasis>.<break/><mark name="1"/>Then <sub alias="doctor">Dr.</sub> Lee came.',
      ),
    );
    expect(spoken.text).toBe("It was dark. Then doctor Lee came.");
    expect(spoken.marks).toEqual([
      { offset: 0, name: "0" },
      { offset: 13, name: "1" },
    ]);
    expect(spoken.lang).toBe("en");
  });

  it("says nothing for something that isn't SSML", () => {
    expect(ssmlToText("<speak><unclosed></speak>").text).toBe("");
  });

  it("finds the sentence being read", () => {
    const marks = [
      { offset: 0, name: "a" },
      { offset: 10, name: "b" },
    ];
    expect(markAt(marks, 3)).toBe("a");
    expect(markAt(marks, 10)).toBe("b");
    expect(markAt([], 4)).toBeNull();
  });

  it("goes round the speeds", () => {
    expect(nextRate(1)).toBe(1.25);
    expect(nextRate(2)).toBe(0.8);
    expect(nextRate(3)).toBe(0.8);
  });
});

/** A voice that says each utterance only when told to. */
function fakeVoice() {
  const said: SpeechSynthesisUtterance[] = [];
  const voice: Voice = {
    speak: (u) => said.push(u),
    cancel: vi.fn(),
  };
  const finish = (u: SpeechSynthesisUtterance) =>
    u.onend?.(new Event("end") as SpeechSynthesisEvent);
  return { voice, said, finish };
}

const utterance = (text: string) =>
  ({ text, rate: 1, lang: "" }) as unknown as SpeechSynthesisUtterance;

const flush = () => new Promise((done) => setTimeout(done, 0));

function source(blocks: string[]) {
  let at = 0;
  const marks: string[] = [];
  const src: SpeechSource = {
    start: async () => blocks[(at = 0)],
    resume: () => blocks[at],
    next: async () => blocks[++at],
    mark: (name) => marks.push(name),
    stopped: vi.fn(),
  };
  return { src, marks };
}

describe("Speaker", () => {
  it("reads block after block, lighting each sentence, and stops at the end", async () => {
    const { voice, said, finish } = fakeVoice();
    const { src, marks } = source([
      SSML('<mark name="0"/>One. <mark name="1"/>Two.'),
      SSML('<mark name="0"/>'), // an image alone: nothing to say
      SSML('<mark name="0"/>Three.'),
    ]);
    const states: string[] = [];
    const speaker = new Speaker(src, (s) => states.push(s), voice, utterance);

    await speaker.play();
    expect(said.map((u) => u.text)).toEqual(["One. Two."]);
    said[0].onboundary?.({ charIndex: 5 } as SpeechSynthesisEvent);
    expect(marks).toEqual(["0", "1"]);

    finish(said[0]);
    await flush();
    expect(said.map((u) => u.text)).toEqual(["One. Two.", "Three."]);

    finish(said[1]);
    await flush();
    expect(states).toEqual(["playing", "stopped"]);
    expect(src.stopped).toHaveBeenCalled();
  });

  it("pauses without running on, and resumes the same sentence at the new speed", async () => {
    const { voice, said, finish } = fakeVoice();
    const { src } = source([SSML("First."), SSML("Second.")]);
    const speaker = new Speaker(src, () => {}, voice, utterance);

    await speaker.play();
    speaker.pause();
    // The cancelled utterance still ends; that mustn't move on.
    finish(said[0]);
    await flush();
    expect(said).toHaveLength(1);

    speaker.rate = 1.5;
    await speaker.play();
    expect(said.map((u) => [u.text, u.rate])).toEqual([
      ["First.", 1],
      ["First.", 1.5],
    ]);
  });
});
