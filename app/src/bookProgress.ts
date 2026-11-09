import { useCallback, useRef, useState } from "react";
import type { BookProgress, ReportBookProgress } from "./api";

export interface BookOperation {
  label: string;
  starting: string;
  progress?: BookProgress;
}

/** Owns the indicator until the command settles, including its initial wait. */
export function useBookProgress() {
  const [operation, setOperation] = useState<BookOperation | null>(null);
  const running = useRef(false);
  const run = useCallback(
    async <T>(
      label: string,
      action: (report: ReportBookProgress) => Promise<T>,
      starting = `${label}…`,
    ): Promise<T> => {
      if (running.current)
        throw new Error("A book operation is already running.");
      running.current = true;
      let active = true;
      setOperation({ label, starting });
      try {
        return await action((progress) => {
          if (active) setOperation({ label, starting, progress });
        });
      } finally {
        active = false;
        running.current = false;
        setOperation(null);
      }
    },
    [],
  );
  return [operation, run] as const;
}
