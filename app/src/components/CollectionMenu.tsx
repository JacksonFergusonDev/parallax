import { useEffect, useRef, useState } from "react";
import type { Collection } from "../api";

interface Props {
  collections: Collection[];
  disabled?: boolean;
  label: string;
  onPick: (name: string) => void;
  onNew: () => void;
}

/** A button that opens a list of collections plus "New collection…". */
export function CollectionMenu({
  collections,
  disabled,
  label,
  onPick,
  onNew,
}: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  const choose = (fn: () => void) => () => {
    setOpen(false);
    fn();
  };

  return (
    <div className="menu-root" ref={root}>
      <button
        className="secondary"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      >
        {label} ▾
      </button>
      {open && (
        <div className="menu" role="menu">
          {collections.map((c) => (
            <button
              key={c.name}
              role="menuitem"
              onClick={choose(() => onPick(c.name))}
            >
              {c.name}
              <span className="muted">{c.book_count}</span>
            </button>
          ))}
          {collections.length > 0 && <hr />}
          <button role="menuitem" onClick={choose(onNew)}>
            New collection…
          </button>
        </div>
      )}
    </div>
  );
}
