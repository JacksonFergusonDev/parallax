// Selection state for reviewing series and genre proposals.
import type { Accepted, Proposal } from "./api";

/** What's ticked for one book. */
export interface Choice {
  series: boolean;
  genres: Set<string>;
}

export type Selection = Map<number, Choice>;

/** Proposals worth showing: something to change. */
export const withChanges = (proposals: Proposal[]) =>
  proposals.filter((p) => p.series !== null || p.genres.length > 0);

/** Confident proposals start ticked; unsure ones don't. */
export function initialSelection(proposals: Proposal[]): Selection {
  return new Map(
    proposals.map((p) => [
      p.book_id,
      {
        series: p.series?.confident ?? false,
        genres: new Set(p.genres.filter((g) => g.confident).map((g) => g.name)),
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

/** Ticks or unticks everything. */
export function selectAll(proposals: Proposal[], on: boolean): Selection {
  return new Map(
    proposals.map((p) => [
      p.book_id,
      {
        series: on && p.series !== null,
        genres: new Set(on ? p.genres.map((g) => g.name) : []),
      },
    ]),
  );
}

export function countChanges(sel: Selection): number {
  let n = 0;
  for (const c of sel.values()) n += Number(c.series) + c.genres.size;
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
    return series || genres.length
      ? [{ book_id: p.book_id, series, genres }]
      : [];
  });
}
