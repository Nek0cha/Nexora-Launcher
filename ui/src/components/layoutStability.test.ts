import { describe, expect, it } from "vitest";
import { sizeChangingStateClasses } from "./layoutStability";

describe("sizeChangingStateClasses", () => {
  it("finds state classes that change the size", () => {
    expect(
      sizeChangingStateClasses(
        "selected:border-2 pressed:font-semibold group-data-selected:border-[5px] hover:px-3 data-disabled:h-8",
      ),
    ).toEqual(["selected:border-2", "pressed:font-semibold", "group-data-selected:border-[5px]", "hover:px-3", "data-disabled:h-8"]);
  });

  it("allows colour, background, shadow and outline", () => {
    expect(
      sizeChangingStateClasses(
        "border border-line px-2 selected:border-line-strong selected:bg-surface-3 hover:text-text pressed:bg-surface-3 focus-visible:outline-2 data-disabled:opacity-50 selected:scale-100",
      ),
    ).toEqual([]);
  });
});
