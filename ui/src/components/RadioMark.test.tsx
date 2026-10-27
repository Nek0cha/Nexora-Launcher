import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { RadioMark } from "./RadioMark";
import { sizeChangingStateClasses } from "./layoutStability";

afterEach(cleanup);

describe("RadioMark", () => {
  it("changes no border width or size by state", () => {
    const { container } = render(<RadioMark />);
    const mark = container.firstElementChild as HTMLElement;
    expect(mark.getAttribute("aria-hidden")).toBe("true");
    expect(sizeChangingStateClasses(mark.className)).toEqual([]);
  });

  it("always renders the dot, hidden until selected, with a size-stable frame", () => {
    const { container } = render(<RadioMark />);
    const dot = container.querySelector("[data-radio-dot]") as HTMLElement;
    expect(dot).not.toBeNull();
    expect(dot.getAttribute("aria-hidden")).toBe("true");
    expect(dot.className).toContain("group-data-selected:scale-100");
    expect(sizeChangingStateClasses(dot.className)).toEqual([]);
  });
});
