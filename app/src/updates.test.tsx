// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { UpdateInfo } from "./api";
import { UpdateNotice } from "./components/UpdateNotice";
import { downloadedShare, type UpdateStage } from "./updates";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => false,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

(
  globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

const update: UpdateInfo = { version: "0.2.0", current: "0.1.0", notes: null };

describe("downloadedShare", () => {
  it("is the fraction downloaded, or null without a size", () => {
    expect(downloadedShare({ downloaded: 25, total: 100 })).toBe(0.25);
    expect(downloadedShare({ downloaded: 120, total: 100 })).toBe(1);
    expect(downloadedShare({ downloaded: 25, total: null })).toBeNull();
  });
});

describe("UpdateNotice", () => {
  let container: HTMLDivElement;
  let root: Root;
  beforeEach(() => {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });
  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  const show = (stage: UpdateStage, busy = false) => {
    const calls = { install: 0, skip: 0 };
    act(() =>
      root.render(
        <UpdateNotice
          update={update}
          stage={stage}
          busy={busy}
          onInstall={() => calls.install++}
          onSkip={() => calls.skip++}
          onClose={() => {}}
        />,
      ),
    );
    return calls;
  };
  const button = (label: string) =>
    [...container.querySelectorAll("button")].find(
      (b) => b.textContent === label,
    )!;

  it("offers the new version and installs on request", () => {
    const calls = show({ stage: "offered" });
    expect(container.textContent).toContain("Parallax 0.2.0 is available");
    expect(container.textContent).toContain("You have 0.1.0.");
    act(() => button("Install and Restart").click());
    expect(calls.install).toBe(1);
  });

  it("waits for a Kobo or library task before restarting", () => {
    show({ stage: "offered" }, true);
    expect(button("Install and Restart").disabled).toBe(true);
    expect(container.textContent).toContain("once the current task finishes");
  });

  it("shows download progress and can't be dismissed mid-install", () => {
    show({ stage: "installing", share: 0.42 });
    expect(container.textContent).toContain("Downloading the update… 42%");
    expect(button("Skip This Version").disabled).toBe(true);
    expect(
      container.querySelector<HTMLButtonElement>('[aria-label="Close"]')!
        .disabled,
    ).toBe(true);
  });

  it("offers a retry when installing fails", () => {
    show({ stage: "failed", error: "signature mismatch" });
    expect(container.textContent).toContain("signature mismatch");
    expect(button("Try Again").disabled).toBe(false);
  });
});
