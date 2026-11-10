// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { DeviceInfo } from "./api";
import { KoboNotice } from "./components/KoboNotice";
import {
  dismissedModels,
  koboNotice,
  koboStanding,
  useKoboNotice,
  withModel,
} from "./koboNotice";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => false,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const kobo = (
  model: string,
  tested: boolean,
  read_only: string | null = null,
): DeviceInfo => ({
  kind: "kobo",
  model,
  firmware: "4.38.0",
  serial: "N1",
  mount: "/Volumes/KOBOeReader",
  capabilities: {
    native_collections: true,
    series: true,
    reading_progress: true,
    reading_time: true,
  },
  screen: { width: 1072, height: 1448, colour: false, source: "model" },
  read_only,
  tested,
});

describe("what Parallax says about a Kobo", () => {
  it("says nothing about a tested model", () => {
    expect(koboStanding(kobo("Kobo Libra 2", true))).toBe("tested");
    expect(koboNotice(kobo("Kobo Libra 2", true))).toBeNull();
  });

  it("explains an untested model it will still change", () => {
    const notice = koboNotice(kobo("Kobo Clara BW", false))!;
    expect(notice.title).toBe(
      "Parallax hasn't been tested on a Kobo Clara BW yet",
    );
    expect(notice.body).toMatch(/backs up the Kobo before every change/);
  });

  it("puts what it won't change ahead of whether it was tested", () => {
    const device = kobo(
      "Kobo Libra 2",
      true,
      "firmware 5 stores data differently",
    );
    expect(koboStanding(device)).toBe("read_only");
    expect(koboNotice(device)!.body).toMatch(
      /organising are off: firmware 5 stores data differently\.$/,
    );
  });
});

describe("remembering dismissed models", () => {
  it("reads one model per line", () => {
    expect(dismissedModels(null).size).toBe(0);
    expect([...dismissedModels("Kobo Clara BW\n\nKobo Sage \n")]).toEqual([
      "Kobo Clara BW",
      "Kobo Sage",
    ]);
  });

  it("adds a model once, dropping the oldest past the limit", () => {
    expect(withModel(null, "Kobo Sage")).toBe("Kobo Sage");
    expect(withModel("Kobo Sage\nKobo Nia", "Kobo Sage")).toBe(
      "Kobo Nia\nKobo Sage",
    );
    expect(withModel("Kobo Sage\nKobo Nia", "Kobo Elipsa", 20)).toBe(
      "Kobo Nia\nKobo Elipsa",
    );
  });
});

describe("the notice in the main window", () => {
  let root: Root;
  let host: HTMLDivElement;
  let stored: Record<string, string | null>;

  function Window({ device }: { device: DeviceInfo | null }) {
    const { notice, standing, close, dismiss } = useKoboNotice(device);
    return notice ? (
      <KoboNotice
        notice={notice}
        standing={standing}
        onClose={close}
        onDismiss={dismiss}
      />
    ) : null;
  }
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 6; i++) await Promise.resolve();
    });
  const render = async (device: DeviceInfo | null) => {
    await act(async () => root.render(<Window device={device} />));
    await settle();
  };
  const click = async (name: string) => {
    const button = [...host.querySelectorAll("button")].find(
      (b) => b.textContent === name || b.getAttribute("aria-label") === name,
    )!;
    await act(async () => button.click());
    await settle();
  };

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    stored = {};
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      if (command === "setting_get") return stored[a.key as string] ?? null;
      if (command === "setting_set") {
        stored[a.key as string] = a.value as string | null;
        return null;
      }
      if (command === "open_help") return null;
      throw new Error(`unexpected ${command}`);
    });
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("closes until the Kobo reconnects", async () => {
    const clara = kobo("Kobo Clara BW", false);
    await render(clara);
    expect(host.textContent).toContain("hasn't been tested");
    await click("Supported Models");
    expect(invoke).toHaveBeenCalledWith("open_help", { page: "devices" });

    await click("Close");
    expect(host.textContent).toBe("");
    await render(clara);
    expect(host.textContent).toBe("");

    // A new connection is a new chance to see it.
    await render(null);
    await render(kobo("Kobo Clara BW", false));
    expect(host.textContent).toContain("hasn't been tested");
    expect(stored.kobo_notices).toBeUndefined();
  });

  it("stays away for that model once dismissed", async () => {
    await render(kobo("Kobo Clara BW", false));
    await click("Don't Show Again");
    expect(stored.kobo_notices).toBe("Kobo Clara BW");
    await render(null);
    await render(kobo("Kobo Clara BW", false));
    expect(host.textContent).toBe("");
    // Other models still get theirs.
    await render(kobo("Kobo Sage", false));
    expect(host.textContent).toContain("Kobo Sage");
  });

  it("is quiet for a tested Kobo or none", async () => {
    await render(null);
    await render(kobo("Kobo Libra 2", true));
    expect(host.textContent).toBe("");
  });
});
