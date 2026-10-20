import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import "../i18n";
import { MemorySlider } from "./MemorySlider";

describe("MemorySlider", () => {
  it("scales_an_inner_disc_not_the_positioned_thumb", () => {
    render(<MemorySlider label="Memory" value={2048} min={512} max={8192} step={256} onChange={() => {}} />);
    const disc = screen.getByTestId("memory-slider-disc");
    // React Aria は thumb に translate(-50%, -50%) を直接付けるため、そこに scale を置くと中心からずれる
    expect(disc.className).toContain("group-data-hovered:scale-110");
    const thumb = disc.parentElement as HTMLElement;
    expect(thumb.style.transform).toBe("translate(-50%, -50%)");
    expect(thumb.className).not.toMatch(/scale-/);
  });
});
