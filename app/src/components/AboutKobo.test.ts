import { describe, expect, it } from "vitest";
import { screenText } from "./AboutKobo";

describe("screenText", () => {
  it("states a known model's screen plainly", () => {
    expect(
      screenText({ width: 1264, height: 1680, colour: false, source: "model" }),
    ).toBe("1264 × 1680, black and white");
  });

  it("says when the size was measured or assumed", () => {
    expect(
      screenText({ width: 1072, height: 1448, colour: true, source: "covers" }),
    ).toBe("1072 × 1448, colour (measured from its covers)");
    expect(
      screenText({
        width: 1072,
        height: 1448,
        colour: false,
        source: "assumed",
      }),
    ).toBe(
      "1072 × 1448, black and white (assumed, as this model isn't known yet)",
    );
  });
});
