import { useEffect, useRef, useState } from "react";

interface Props {
  title: string;
  initial?: string;
  confirmLabel: string;
  onSubmit: (name: string) => Promise<void>;
  onClose: () => void;
}

/** A small modal asking for a name (new or renamed collection). */
export function NameDialog({
  title,
  initial = "",
  confirmLabel,
  onSubmit,
  onClose,
}: Props) {
  const [name, setName] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.select();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await onSubmit(name.trim());
      onClose();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <form
        className="modal small"
        onSubmit={submit}
        onClick={(e) => e.stopPropagation()}
      >
        <h2>{title}</h2>
        <input
          ref={input}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
        />
        {error && <p className="notice notice-error">{error}</p>}
        <div className="form-actions">
          <button type="button" className="secondary" onClick={onClose}>
            Cancel
          </button>
          <button
            type="submit"
            className="primary"
            disabled={busy || !name.trim()}
          >
            {confirmLabel}
          </button>
        </div>
      </form>
    </div>
  );
}
