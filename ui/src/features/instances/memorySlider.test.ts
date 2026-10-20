import { describe, expect, it } from "vitest";
import { sliderMax, toSliderValue } from "./memorySlider";

describe("memorySlider", () => {
  it("sliderMax_floors_to_step_and_clamps", () => {
    expect(sliderMax(8192)).toBe(8192);
    expect(sliderMax(8200)).toBe(8192);
    expect(sliderMax(300)).toBe(512);
    expect(sliderMax(200000)).toBe(65536);
  });

  it("toSliderValue_pins_to_max", () => {
    expect(toSliderValue(32768, 8192)).toBe(8192);
    expect(toSliderValue(4096, 8192)).toBe(4096);
  });
});
