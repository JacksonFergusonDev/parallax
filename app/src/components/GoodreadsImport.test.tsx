// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { GoodreadsImport } from "./GoodreadsImport";
import type { GoodreadsPlan } from "../api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const quiet: GoodreadsPlan = {
  added: 2,
  updated: 1,
  unchanged: 5,
  skipped: 0,
  conflicts: [],
  missing: [],
};
const asking: GoodreadsPlan = {
  ...quiet,
  conflicts: [
    {
      goodreads_id: "11",
      title: "Dune",
      authors: ["Frank Herbert"],
      changes: [
        {
          field: "rating",
          label: "Rating",
          mine: "4 stars",
          theirs: "2 stars",
        },
      ],
    },
  ],
  missing: [
    {
      goodreads_id: "22",
      title: "Emma",
      authors: ["Jane Austen"],
      status: "Read",
    },
    {
      goodreads_id: "33",
      title: "Ulysses",
      authors: [],
      status: "Want to read",
    },
  ],
};
const report = {
  added: 2,
  updated: 1,
  unchanged: 5,
  skipped: 0,
  kept: 2,
  removed: 0,
  kept_yours: 1,
  took_goodreads: 0,
};

describe("GoodreadsImport", () => {
  let root: Root;
  let host: HTMLDivElement;
  const onClose = vi.fn();
  let plan = quiet;

  const button = (text: string) =>
    [...host.querySelectorAll("button")].find((b) =>
      b.textContent?.includes(text),
    ) as HTMLButtonElement;
  const click = (b: HTMLElement) =>
    act(async () => {
      b.click();
      await Promise.resolve();
      await Promise.resolve();
    });
  const imports = () =>
    vi.mocked(invoke).mock.calls.filter(([c]) => c === "goodreads_import");

  const start = async (p: GoodreadsPlan) => {
    plan = p;
    await act(async () => {
      root.render(<GoodreadsImport onClose={onClose} />);
    });
    await click(button("Choose export"));
  };

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
    vi.mocked(open).mockResolvedValue("/Downloads/export.csv");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "goodreads_plan") return plan;
      if (command === "goodreads_import") return report;
      throw new Error(`unexpected ${command}`);
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.clearAllMocks();
  });

  it("imports without questions when nothing needs a decision", async () => {
    await start(quiet);
    expect(host.textContent).toContain("2 books will be added");
    expect(host.textContent).not.toContain("need your decision");
    await click(button("Import"));
    expect(imports()).toEqual([
      [
        "goodreads_import",
        {
          path: "/Downloads/export.csv",
          resolutions: { conflicts: {}, missing: {} },
        },
      ],
    ]);
    expect(host.textContent).toContain("2 books kept as your own");
    expect(button("Done")).toBeTruthy();
  });

  it("lays out each conflict and missing book in plain words", async () => {
    await start(asking);
    expect(host.textContent).toContain("3 books need your decision");
    expect(host.textContent).toContain(
      "Rating: you have 4 stars, Goodreads has 2 stars",
    );
    expect(host.textContent).toContain("2 books not in this export");
    expect(host.textContent).not.toMatch(/differs|conflict|missing flag/i);
  });

  it("takes the recommended choices in one click", async () => {
    await start(asking);
    await click(button("Use recommended"));
    expect(imports()[0][1]).toEqual({
      path: "/Downloads/export.csv",
      resolutions: { conflicts: {}, missing: {} },
    });
  });

  it("sends exactly what was chosen, and says when books will be removed", async () => {
    await start(asking);
    await click(button("Use Goodreads"));
    await click(button("Remove all"));
    expect(button("Import and remove 2 books")).toBeTruthy();
    await click(button("Import and remove 2 books"));
    expect(imports()[0][1]).toEqual({
      path: "/Downloads/export.csv",
      resolutions: {
        conflicts: { "11": "goodreads" },
        missing: { "22": "remove", "33": "remove" },
      },
    });
  });

  it("starts with the recommended choices selected", async () => {
    await start(asking);
    const checked = [...host.querySelectorAll('[aria-checked="true"]')].map(
      (b) => b.textContent,
    );
    expect(checked).toEqual(["Keep mine", "Keep", "Keep"]);
  });

  it("shows an error and stays open when the file isn't an export", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(
      "this doesn't look like a Goodreads export",
    );
    await act(async () => {
      root.render(<GoodreadsImport onClose={onClose} />);
    });
    await click(button("Choose export"));
    expect(host.textContent).toContain("doesn't look like a Goodreads export");
    expect(button("Choose export")).toBeTruthy();
  });
});
