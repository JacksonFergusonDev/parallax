// Telling the owner of an untested or read-only Kobo what that means, once
// per model: in setup if it's running, otherwise in the main window.
import { useCallback, useEffect, useState } from "react";
import {
  getSetting,
  onSettingChange,
  setSetting,
  type DeviceInfo,
} from "./api";

export type KoboStanding = "tested" | "untested" | "read_only";

/** Whether Parallax has hardware checks for this Kobo, or won't change it. */
export function koboStanding(device: DeviceInfo): KoboStanding {
  if (device.read_only !== null) return "read_only";
  return device.tested ? "tested" : "untested";
}

/** What to tell the owner, or null for a tested Kobo. */
export function koboNotice(
  device: DeviceInfo,
): { title: string; body: string } | null {
  switch (koboStanding(device)) {
    case "tested":
      return null;
    case "untested":
      return {
        title: `Parallax hasn't been tested on a ${device.model} yet`,
        body: "Its database matches what Parallax expects, so sending, updating and organising books are on. Parallax backs up the Kobo before every change, and Revert Kobo Changes… undoes one. If anything looks wrong, please report it from the Help menu.",
      };
    case "read_only":
      return {
        title: `Parallax can't change this ${device.model} yet`,
        body: `You can browse its books and add them to your library, but sending, updating and organising are off: ${device.read_only}.`,
      };
  }
}

/** The models whose notice was dismissed, from the `kobo_notices` setting. */
export const dismissedModels = (saved: string | null): Set<string> =>
  new Set(
    (saved ?? "")
      .split("\n")
      .map((model) => model.trim())
      .filter(Boolean),
  );

/** The setting with `model` added, dropping the oldest past the limit. */
export function withModel(saved: string | null, model: string, max = 2000) {
  const models = [...dismissedModels(saved)].filter((m) => m !== model);
  models.push(model);
  while (models.length > 1 && models.join("\n").length > max) models.shift();
  return models.join("\n");
}

/** Remembers that the owner has seen the notice for `model`. */
export async function rememberKoboNotice(model: string) {
  const saved = await getSetting("kobo_notices");
  if (dismissedModels(saved).has(model)) return;
  await setSetting("kobo_notices", withModel(saved, model));
}

/**
 * The notice to show for `device` in the main window, if any. Closing it
 * hides it until the Kobo reconnects; dismissing it does so for the model.
 */
export function useKoboNotice(device: DeviceInfo | null) {
  const [dismissed, setDismissed] = useState<Set<string> | null>(null);
  /** The connection the notice was closed for. */
  const [closed, setClosed] = useState<DeviceInfo | null>(null);

  useEffect(() => {
    let alive = true;
    getSetting("kobo_notices")
      .then((saved) => alive && setDismissed(dismissedModels(saved)))
      .catch(() => alive && setDismissed(new Set()));
    const unlisten = onSettingChange(({ key, value }) => {
      if (key === "kobo_notices") setDismissed(dismissedModels(value));
    });
    return () => {
      alive = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  const notice =
    device && dismissed && !dismissed.has(device.model) && closed !== device
      ? koboNotice(device)
      : null;

  const close = useCallback(() => setClosed(device), [device]);
  const dismiss = useCallback(() => {
    if (!device) return;
    setDismissed((old) => new Set([...(old ?? []), device.model]));
    void rememberKoboNotice(device.model).catch(console.error);
  }, [device]);

  return { notice, standing: device && koboStanding(device), close, dismiss };
}
