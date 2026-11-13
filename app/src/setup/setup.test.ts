import { describe, expect, it } from "vitest";
import { duringSetup } from "../menu";
import {
  COUNTED,
  customised,
  healthIssues,
  nextStep,
  previousStep,
  setupOnLaunch,
  STEPS,
} from "./setup";

describe("when setup shows", () => {
  it("shows on a first launch and after an unfinished one", () => {
    expect(setupOnLaunch(null, 0)).toBe("show");
    expect(setupOnLaunch("started", 0)).toBe("show");
    // Books added during an unfinished setup don't end it.
    expect(setupOnLaunch("started", 40)).toBe("show");
  });

  it("stays away once skipped or done", () => {
    expect(setupOnLaunch("skipped", 0)).toBe("hide");
    expect(setupOnLaunch("done", 0)).toBe("hide");
  });

  it("quietly finishes for a library from before setup existed", () => {
    expect(setupOnLaunch(null, 1)).toBe("adopt");
  });
});

describe("setup steps", () => {
  it("runs from welcome to done and stops at either end", () => {
    expect(STEPS[0]).toBe("welcome");
    expect(STEPS.at(-1)).toBe("done");
    expect(previousStep("welcome")).toBe("welcome");
    expect(nextStep("done")).toBe("done");
    expect(nextStep("connect")).toBe("books");
    expect(previousStep("books")).toBe("connect");
  });

  it("counts every step but the welcome and the finish", () => {
    expect(COUNTED).toEqual([
      "connect",
      "books",
      "goodreads",
      "settings",
      "appearance",
      "tour",
    ]);
  });
});

describe("recommended settings", () => {
  it("are every offered setting left unset", () => {
    expect(customised({})).toEqual([]);
    expect(customised({ eject_after_send: null, theme: "dark" })).toEqual([]);
    expect(customised({ open_view: "last", comics_kind: "comic" })).toEqual([
      "comics_kind",
      "open_view",
    ]);
  });
});

describe("Kobo health during setup", () => {
  const healthy = {
    missing_files: [],
    not_imported: [],
    duplicate_collections: [],
    orphaned_images: 0,
    orphaned_image_bytes: 0,
  };

  it("doesn't count books the Kobo has yet to import", () => {
    const pending = { path: "a.kepub.epub", content_id: "a", title: "A" };
    expect(healthIssues({ ...healthy, not_imported: [pending] })).toBe(0);
  });

  it("counts each kind of tidying once per item, images as one", () => {
    const missing = { path: "b.kepub.epub", content_id: "b", title: "B" };
    expect(
      healthIssues({
        ...healthy,
        missing_files: [missing, missing],
        duplicate_collections: [{ name: "Favourites", copies: 2 }],
        orphaned_images: 23,
      }),
    ).toBe(4);
  });
});

describe("the menu during setup", () => {
  it("keeps only commands that make sense over setup", () => {
    const run = () => {};
    const commands = duringSetup({
      "file.add": { run },
      "file.calibre": { run },
      "kobo.eject": { run, enabled: false },
      "view.library": { run, checked: true },
      "help.welcome": { run },
    });
    expect(commands["file.add"].enabled).toBeUndefined();
    expect(commands["file.calibre"].enabled).toBeUndefined();
    expect(commands["kobo.eject"].enabled).toBe(false);
    expect(commands["view.library"]).toMatchObject({
      enabled: false,
      checked: true,
    });
    expect(commands["help.welcome"].enabled).toBe(false);
  });
});
