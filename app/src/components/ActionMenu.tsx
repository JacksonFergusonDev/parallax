import { useEffect, useRef, useState } from "react";

interface Props {
  label: string;
  icon?: React.ReactNode;
  disabled?: boolean;
  items: { label: string; onPick: () => void }[];
}

/** A button that opens a short list of actions. */
export function ActionMenu({ label, icon, disabled, items }: Props) {
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

  return (
    <div className="menu-root" ref={root}>
      <button
        className="secondary"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      >
        {icon}
        {label} ▾
      </button>
      {open && (
        <div className="menu" role="menu">
          {items.map((item) => (
            <button
              key={item.label}
              role="menuitem"
              onClick={() => {
                setOpen(false);
                item.onPick();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
