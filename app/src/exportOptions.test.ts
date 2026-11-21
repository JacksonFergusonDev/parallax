import { describe, expect, it } from "vitest";
import {
  DEFAULT_CHOICES,
  ORGANIZE,
  describeChoices,
  parseChoices,
  templateFor,
  templateProblem,
} from "./exportOptions";

describe("export options", () => {
  it("default to one ZIP file of EPUBs in author folders", () => {
    expect(parseChoices(null)).toEqual(DEFAULT_CHOICES);
    expect(describeChoices(DEFAULT_CHOICES)).toBe(
      "One ZIP file with a folder for each author, as EPUB.",
    );
    expect(templateFor(DEFAULT_CHOICES)).toBe(
      "{author}/[{series} {series_index} - ]{title}",
    );
  });

  it("read back saved choices and ignore anything unknown", () => {
    expect(
      parseChoices(
        JSON.stringify({ archive: false, organize: "genre", format: "kepub" }),
      ),
    ).toEqual({
      ...DEFAULT_CHOICES,
      archive: false,
      organize: "genre",
      format: "kepub",
    });
    expect(
      parseChoices(JSON.stringify({ organize: "colour", format: "pdf" })),
    ).toEqual(DEFAULT_CHOICES);
    expect(parseChoices("not json")).toEqual(DEFAULT_CHOICES);
  });

  it("use a custom template only when Custom is chosen", () => {
    const custom = {
      ...DEFAULT_CHOICES,
      organize: "custom" as const,
      template: " {year}/{title} ",
    };
    expect(templateFor(custom)).toBe("{year}/{title}");
    expect(templateFor({ ...custom, organize: "flat" })).toBe(
      "[{author} - ]{title}",
    );
  });

  it("explain what's wrong with a template", () => {
    for (const { template } of ORGANIZE)
      expect(templateProblem(template)).toBeNull();
    expect(templateProblem("books")).toMatch(/at least one field/);
    expect(templateProblem("{name}")).toMatch(/\{name\} isn't a field/);
    expect(templateProblem("{title")).toMatch(/Close each/);
  });
});
