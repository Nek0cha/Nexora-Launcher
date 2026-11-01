import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { EmptyState } from "./EmptyState";

afterEach(cleanup);

describe("EmptyState", () => {
  it("puts a gap between the actions so that side-by-side buttons do not touch", () => {
    render(
      <EmptyState title="Instances">
        <button>Create</button>
        <button>Import</button>
      </EmptyState>,
    );
    const row = screen.getByRole("button", { name: "Create" }).parentElement;
    expect(row?.className).toMatch(/\bflex\b/);
    expect(row?.className).toMatch(/\bgap-2\b/);
    expect(row?.contains(screen.getByRole("button", { name: "Import" }))).toBe(true);
  });
});
