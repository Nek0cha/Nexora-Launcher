import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Skeleton } from "./Skeleton";

afterEach(cleanup);

describe("Skeleton", () => {
  it("is hidden from assistive tech, uses a surface token and pulses only under motion-safe", () => {
    const { container } = render(<Skeleton className="h-4 w-20" />);
    const el = container.firstElementChild as HTMLElement;
    expect(el.getAttribute("aria-hidden")).toBe("true");
    expect(el.className).toContain("bg-surface-2");
    expect(el.className).toContain("motion-safe:animate-pulse");
    expect(el.className).not.toMatch(/(^|\s)animate-pulse/);
    expect(el.className).toContain("h-4 w-20");
  });
});
