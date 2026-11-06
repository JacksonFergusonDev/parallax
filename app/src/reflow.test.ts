import { describe, expect, it } from "vitest";
import { slotDelta } from "./reflow";

describe("slotDelta", () => {
  it("is zero when the slot doesn't change", () => {
    expect(slotDelta(2, 6, 4, 166, 280)).toEqual({ dx: 0, dy: 0 });
  });

  it("points back to the old slot when columns are lost", () => {
    // Index 5: column 5 of row 0 with six columns; column 1 of row 1 with four.
    expect(slotDelta(5, 6, 4, 166, 280)).toEqual({ dx: 4 * 166, dy: -280 });
  });

  it("points back to the old slot when columns are gained", () => {
    // Index 4: column 0 of row 1 with four columns; column 4 of row 0 with six.
    expect(slotDelta(4, 4, 6, 166, 280)).toEqual({ dx: -4 * 166, dy: 280 });
  });
});
