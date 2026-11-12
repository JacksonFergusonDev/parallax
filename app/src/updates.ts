// A newer Parallax: the banner's state, from the update check in
// src-tauri/src/updates.rs. Nothing installs until the user says so, even
// when the update downloaded on its own.
import { useCallback, useEffect, useState } from "react";
import {
  installUpdate,
  onUpdateAvailable,
  onUpdateProgress,
  skipUpdate,
  updateAvailable,
  type UpdateInfo,
  type UpdateProgress,
} from "./api";

/** How far the download is, from 0 to 1, or null when its size is unknown. */
export function downloadedShare({ downloaded, total }: UpdateProgress) {
  return total ? Math.min(1, downloaded / total) : null;
}

export type UpdateStage =
  | { stage: "offered" }
  | { stage: "installing"; share: number | null }
  | { stage: "failed"; error: string };

export function useUpdate() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [stage, setStage] = useState<UpdateStage>({ stage: "offered" });

  useEffect(() => {
    let live = true;
    updateAvailable()
      .then((found) => {
        if (live && found) setUpdate(found);
      })
      .catch(console.error);
    const offered = onUpdateAvailable((found) => {
      setUpdate(found);
      setStage({ stage: "offered" });
    });
    const progress = onUpdateProgress((p) =>
      setStage({ stage: "installing", share: downloadedShare(p) }),
    );
    return () => {
      live = false;
      void offered.then((stop) => stop());
      void progress.then((stop) => stop());
    };
  }, []);

  const install = useCallback(async () => {
    setStage({ stage: "installing", share: 0 });
    try {
      // Parallax restarts once the update is in place.
      await installUpdate();
    } catch (error) {
      setStage({ stage: "failed", error: String(error) });
    }
  }, []);

  const skip = useCallback(async () => {
    if (!update) return;
    await skipUpdate(update.version);
    setUpdate(null);
  }, [update]);

  const close = useCallback(() => setUpdate(null), []);

  return { update, stage, install, skip, close };
}
