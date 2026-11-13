// Charts shared by the Reading screens: stat tiles, a column chart over time
// and horizontal bars for categories. Plain HTML and CSS on the design
// tokens, so they follow every theme.

import { useState } from "react";
import { spineColor } from "../coverArt";
import { ticks, type Bucket } from "../stats";

export function Tile({
  label,
  value,
  note,
}: {
  label: string;
  value: string;
  note?: string;
}) {
  return (
    <div className="stat-tile">
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
      {note && <span className="muted stat-note">{note}</span>}
    </div>
  );
}

/** "1,234" in the user's locale. */
export const num = (n: number) => n.toLocaleString();

/** Axis numbers short enough for the tick column: 900, 1.5k, 12k. */
export function shortNum(n: number): string {
  if (n < 1000) return String(n);
  const k = n / 1000;
  return `${k < 10 && k % 1 ? k.toFixed(1) : Math.round(k)}k`;
}

interface ColumnProps {
  buckets: Bucket[];
  /** What a count is: ["book", "books"]. */
  unit: [string, string];
  /** What the chart shows, for screen readers. */
  label: string;
  /** Draws each count as a book spine while they stay thick enough. */
  spines?: boolean;
}

/** One series of columns: a hover tooltip per column, the tallest labelled. */
export function ColumnChart({
  buckets,
  unit,
  label,
  spines = true,
}: ColumnProps) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(...buckets.map((b) => b.count));
  const axis = ticks(max);
  const top = axis[axis.length - 1];
  const peak = buckets.findIndex((b) => b.count === max && max > 0);
  const stacked = spines && top <= 30;
  const units = (n: number) => `${num(n)} ${n === 1 ? unit[0] : unit[1]}`;
  // Crowded charts label every other column; each column's tooltip names it.
  const every = buckets.length > 8 ? 2 : 1;

  return (
    <div
      className="column-chart"
      role="img"
      aria-label={`${label}: ${buckets.map((b) => `${b.label} ${units(b.count)}`).join(", ")}`}
    >
      <div className="chart-plot">
        {axis.map((t) => (
          <div
            key={t}
            className="gridline"
            style={{ bottom: `${(t / top) * 100}%` }}
          >
            <span className="tick">{shortNum(t)}</span>
          </div>
        ))}
        <div className="columns">
          {buckets.map((b, i) => (
            <div
              key={b.key}
              className="column-slot"
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover(null)}
            >
              {i === peak && hover === null && (
                <span
                  className="column-value"
                  style={{ bottom: `${(b.count / top) * 100}%` }}
                >
                  {num(b.count)}
                </span>
              )}
              {b.count > 0 &&
                (stacked ? (
                  <div
                    className={`spine-stack ${hover === i ? "hovered" : ""}`}
                    style={{ height: `${(b.count / top) * 100}%` }}
                  >
                    {Array.from({ length: b.count }, (_, n) => (
                      <span
                        key={n}
                        className="spine"
                        style={
                          {
                            "--spine": spineColor(`${b.key}:${n}`),
                            "--tilt": `${((n * 7) % 5) - 2}px`,
                            "--w": `${84 + ((n * 13) % 17)}%`,
                          } as React.CSSProperties
                        }
                      />
                    ))}
                  </div>
                ) : (
                  <div
                    className={`column ${hover === i ? "hovered" : ""}`}
                    style={{ height: `${(b.count / top) * 100}%` }}
                  />
                ))}
              {hover === i && (
                <div
                  className="chart-tooltip"
                  style={{ bottom: `calc(${(b.count / top) * 100}% + 8px)` }}
                >
                  <strong>{num(b.count)}</strong>{" "}
                  {b.count === 1 ? unit[0] : unit[1]}
                  <span className="muted"> · {b.tip ?? b.key}</span>
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
      <div className="column-labels">
        {buckets.map((b, i) => (
          <span key={b.key}>
            {(buckets.length - 1 - i) % every === 0 ? b.label : ""}
          </span>
        ))}
      </div>
    </div>
  );
}

export interface Bar {
  key: string;
  label: string;
  value: number;
  /** Shown after the value, such as an average rating. */
  note?: string;
}

/** Labelled horizontal bars, scaled to the largest. Values are always shown,
 * so the bars only need to carry the comparison. */
export function BarList({ bars, label }: { bars: Bar[]; label: string }) {
  const max = Math.max(1, ...bars.map((b) => b.value));
  return (
    <ul className="bar-list" aria-label={label}>
      {bars.map((b) => (
        <li key={b.key} className="bar-row">
          <span className="bar-label" title={b.label}>
            {b.label}
          </span>
          <span className="bar-track" aria-hidden>
            <span
              className="bar-fill"
              style={{ width: `${(b.value / max) * 100}%` }}
            />
          </span>
          <span className="bar-value">
            {num(b.value)}
            {b.note && <span className="muted"> · {b.note}</span>}
          </span>
        </li>
      ))}
    </ul>
  );
}
