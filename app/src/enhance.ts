// Selection state for reviewing series and genre proposals.
import { enhancePropose, type Accepted, type Proposal } from "./api";

/** What's ticked for one book. */
export interface Choice {
  series: boolean;
  genres: Set<string>;
  details?: boolean;
}

export type Selection = Map<number, Choice>;

/** Proposals worth showing: something to change. */
export const withChanges = (proposals: Proposal[]) =>
  proposals.filter(
    (p) =>
      p.series !== null || p.genres.length > 0 || p.details || p.issues?.length,
  );

export async function resolveIssue(
  bookId: number,
  issue: number,
  mount: string | null,
) {
  const [result] = await enhancePropose([bookId], mount, issue);
  if (!result || result.problems.length) {
    throw new Error(result?.problems.join(". ") || "Metron returned no issue.");
  }
  return result;
}

export function chooseResolved(selection: Selection, proposal: Proposal) {
  return new Map([...selection, ...initialSelection([proposal])]);
}

/** Confident proposals start ticked; unsure ones don't. */
export function initialSelection(proposals: Proposal[]): Selection {
  return new Map(
    proposals.map((p) => [
      p.book_id,
      {
        series: p.series?.confident ?? false,
        genres: new Set(p.genres.filter((g) => g.confident).map((g) => g.name)),
        details: p.details?.confident ?? false,
      },
    ]),
  );
}

export function toggleSeries(sel: Selection, id: number): Selection {
  const next = new Map(sel);
  const c = next.get(id) ?? { series: false, genres: new Set<string>() };
  next.set(id, { ...c, series: !c.series });
  return next;
}

export function toggleGenre(
  sel: Selection,
  id: number,
  genre: string,
): Selection {
  const next = new Map(sel);
  const c = next.get(id) ?? { series: false, genres: new Set<string>() };
  const genres = new Set(c.genres);
  if (genres.has(genre)) genres.delete(genre);
  else genres.add(genre);
  next.set(id, { ...c, genres });
  return next;
}

export function toggleDetails(sel: Selection, id: number): Selection {
  const next = new Map(sel);
  const choice: Choice = next.get(id) ?? {
    series: false,
    genres: new Set<string>(),
  };
  next.set(id, { ...choice, details: !choice.details });
  return next;
}

/** Ticks or unticks everything. */
export function selectAll(proposals: Proposal[], on: boolean): Selection {
  return new Map(
    proposals.map((p) => [
      p.book_id,
      {
        series: on && p.series !== null,
        genres: new Set(on ? p.genres.map((g) => g.name) : []),
        details: on && p.details !== undefined,
      },
    ]),
  );
}

export function countChanges(sel: Selection): number {
  let n = 0;
  for (const c of sel.values())
    n += Number(c.series) + c.genres.size + Number(c.details ?? false);
  return n;
}

export function toAccepted(proposals: Proposal[], sel: Selection): Accepted[] {
  return proposals.flatMap((p) => {
    const c = sel.get(p.book_id);
    if (!c) return [];
    const series =
      c.series && p.series
        ? ([p.series.name, p.series.index] as [string, string | null])
        : null;
    const genres = p.genres
      .filter((g) => c.genres.has(g.name))
      .map((g) => g.name);
    const details = c.details ? p.details : undefined;
    if (!series && !genres.length && !details) return [];
    const label = p.record?.catalog === "mangainfo" ? "Mangainfo" : "Metron";
    const record =
      p.record &&
      (details ||
        (series && p.series?.source === label) ||
        p.genres.some((g) => c.genres.has(g.name) && g.sources.includes(label)))
        ? p.record
        : undefined;
    return [
      {
        book_id: p.book_id,
        series,
        genres,
        ...(details && { details }),
        ...(record && { record }),
      },
    ];
  });
}
