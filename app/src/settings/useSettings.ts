import { useCallback, useEffect, useState } from "react";
import {
  allSettings,
  onReaderSettingsChange,
  onSettingChange,
  readerSettings,
  setReaderSettings,
  setSetting,
  type ReaderSettings,
  type SettingKey,
} from "../api";

export type Values = Partial<Record<SettingKey, string | null>>;

/**
 * Every setting, kept current with changes from other windows. Changes show
 * at once and are saved straight away; a refused save puts the value back.
 */
export function useSettings(onError: (message: string) => void) {
  const [values, setValues] = useState<Values | null>(null);
  const [reader, setReader] = useState<ReaderSettings | null>(null);

  useEffect(() => {
    let alive = true;
    Promise.all([allSettings(), readerSettings()])
      .then(([all, appearance]) => {
        if (!alive) return;
        setValues(all);
        setReader(appearance);
      })
      .catch((e) => onError(String(e)));
    const unlisten = [
      onSettingChange(({ key, value }) =>
        setValues((old) => (old ? { ...old, [key]: value } : old)),
      ),
      onReaderSettingsChange(setReader),
    ];
    return () => {
      alive = false;
      for (const stop of unlisten) void stop.then((f) => f());
    };
  }, [onError]);

  const set = useCallback(
    (key: SettingKey, value: string | null) => {
      let before: string | null = null;
      setValues((old) => {
        before = old?.[key] ?? null;
        return { ...old, [key]: value };
      });
      setSetting(key, value).catch((e) => {
        setValues((old) => ({ ...old, [key]: before }));
        onError(String(e));
      });
    },
    [onError],
  );

  const setAppearance = useCallback(
    (next: ReaderSettings) => {
      setReader(next);
      setReaderSettings(next).catch((e) => onError(String(e)));
    },
    [onError],
  );

  return { values, set, reader, setAppearance };
}
