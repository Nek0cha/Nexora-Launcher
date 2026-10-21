import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StableLabel } from "./StableLabel";

afterEach(cleanup);

describe("StableLabel", () => {
  it("keeps every label in the layout so the width does not change, and hides the inactive ones", () => {
    const { rerender } = render(<StableLabel labels={["Add", "Adding...", "Installed"]} active={0} />);
    // 全部の文字が場所を取る（幅が最長に合う）。見えない文字は読み上げから外れる
    expect(screen.getAllByText(/Add|Adding\.\.\.|Installed/)).toHaveLength(3);
    expect(screen.getByText("Add").getAttribute("aria-hidden")).toBeNull();
    expect(screen.getByText("Adding...").getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByText("Adding...").className).toContain("invisible");

    rerender(<StableLabel labels={["Add", "Adding...", "Installed"]} active={1} />);
    expect(screen.getByText("Adding...").getAttribute("aria-hidden")).toBeNull();
    expect(screen.getByText("Add").getAttribute("aria-hidden")).toBe("true");
  });
});
