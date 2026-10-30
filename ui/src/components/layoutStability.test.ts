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

  it("catches arbitrary variants and the other state prefixes", () => {
    expect(
      sizeChangingStateClasses(
        "data-[state=open]:border-2 group-data-[selected=true]:px-3 aria-[pressed=true]:font-bold active:h-8 open:p-2 pending:w-4 invalid:border-[3px] expanded:gap-2 drop-target:border-4 motion-safe:selected:px-1",
      ),
    ).toEqual([
      "data-[state=open]:border-2",
      "group-data-[selected=true]:px-3",
      "aria-[pressed=true]:font-bold",
      "active:h-8",
      "open:p-2",
      "pending:w-4",
      "invalid:border-[3px]",
      "expanded:gap-2",
      "drop-target:border-4",
      "motion-safe:selected:px-1",
    ]);
  });

  it("allows ring and outline, and variants that are not states", () => {
    expect(
      sizeChangingStateClasses(
        "selected:ring-2 focus-visible:ring-accent data-[state=open]:ring-2 focus-visible:outline-2 motion-safe:px-2 @lg:px-2 md:h-8 data-[state=open]:bg-surface-3",
      ),
    ).toEqual([]);
  });
});
