import type { ExportRequest } from "./api";

/** Ways to organise an export. Templates match PRESETS in
 * crates/core/src/export.rs. */
export const ORGANIZE = [
  {
    id: "author",
    label: "Author, series in the name",
    template: "{author}/[{series} {series_index} - ]{title}",
  },
  {
    id: "author-series",
    label: "Author, then series",
    template: "{author}/[{series}/][{series_index} - ]{title}",
  },
  {
    id: "genre",
    label: "Genre, then author",
    template: "{genre}/{author}/[{series} {series_index} - ]{title}",
  },
  {
    id: "series",
    label: "Series",
    template: "[{series}/][{series_index} - ]{title}",
  },
  { id: "flat", label: "No folders", template: "[{author} - ]{title}" },
] as const;

export type OrganizeId = (typeof ORGANIZE)[number]["id"] | "custom";

/** What Export Library remembers between exports. Your settings are left
 * out each time unless you ask again. */
export interface ExportChoices {
  archive: boolean;
  organize: OrganizeId;
  /** The custom template, kept when you switch to a preset. */
  template: string;
  format: ExportRequest["format"];
}

export const DEFAULT_CHOICES: ExportChoices = {
  archive: true,
  organize: "author",
  template: ORGANIZE[0].template,
  format: "epub",
};

/** The fields a template may use. Matches FIELDS in export.rs. */
export const FIELDS = [
  "title",
  "author",
  "authors",
  "series",
  "series_index",
  "genre",
  "publisher",
  "year",
  "language",
  "id",
];

/** Why a template can't be used, or null when it can. */
export function templateProblem(template: string): string | null {
  const fields = [...template.matchAll(/\{([^{}]*)\}/g)].map((m) => m[1]);
  if (template.split("{").length - 1 !== fields.length)
    return "Close each { with a }.";
  const unknown = fields.find((f) => !FIELDS.includes(f));
  if (unknown !== undefined) return `{${unknown}} isn't a field you can use.`;
  if (fields.length === 0) return "Use at least one field, such as {title}.";
  return null;
}

/** The template `choices` export with. */
export function templateFor(choices: ExportChoices): string {
  return (
    ORGANIZE.find((o) => o.id === choices.organize)?.template ??
    choices.template.trim()
  );
}

/** Saved choices, with anything missing or unknown at its default. */
export function parseChoices(saved: string | null | undefined): ExportChoices {
  let value: Partial<ExportChoices> = {};
  try {
    const parsed: unknown = saved ? JSON.parse(saved) : {};
    if (parsed && typeof parsed === "object") value = parsed;
  } catch {
    // An unreadable value is the default.
  }
  const organize =
    value.organize === "custom" || ORGANIZE.some((o) => o.id === value.organize)
      ? (value.organize as OrganizeId)
      : DEFAULT_CHOICES.organize;
  return {
    archive:
      typeof value.archive === "boolean"
        ? value.archive
        : DEFAULT_CHOICES.archive,
    organize,
    template:
      typeof value.template === "string" && value.template.trim()
        ? value.template
        : DEFAULT_CHOICES.template,
    format: value.format === "kepub" ? "kepub" : "epub",
  };
}

/** One line saying what an export with `choices` makes. */
export function describeChoices(choices: ExportChoices): string {
  const container = choices.archive ? "One ZIP file" : "A folder";
  const organize =
    choices.organize === "custom"
      ? "named by your template"
      : {
          author: "with a folder for each author",
          "author-series": "with a folder for each author and series",
          genre: "with a folder for each genre",
          series: "with a folder for each series",
          flat: "with no folders inside",
        }[choices.organize];
  const format = choices.format === "kepub" ? "KEPUB" : "EPUB";
  return `${container} ${organize}, as ${format}.`;
}
