import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ReadyGate } from "./ReadyGate";

const q = (isPending: boolean) => ({ isPending });

function Gate({ queries }: { queries: { isPending: boolean }[] }) {
  return (
    <ReadyGate queries={queries} skeleton={<p>skeleton</p>}>
      <p>content</p>
    </ReadyGate>
  );
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const tick = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

describe("ReadyGate", () => {
  it("shows only the content when every query has settled", () => {
    render(<Gate queries={[q(false), q(false)]} />);
    expect(screen.getByText("content")).toBeTruthy();
    expect(screen.queryByText("skeleton")).toBeNull();
  });

  it("shows the content at once for an empty list", () => {
    render(<Gate queries={[]} />);
    expect(screen.getByText("content")).toBeTruthy();
  });

  it("shows nothing for the first 150ms, then the skeleton", () => {
    render(<Gate queries={[q(true), q(false)]} />);
    expect(screen.queryByText("content")).toBeNull();
    expect(screen.queryByText("skeleton")).toBeNull();
    tick(149);
    expect(screen.queryByText("skeleton")).toBeNull();
    tick(1);
    expect(screen.getByText("skeleton")).toBeTruthy();
    expect(screen.queryByText("content")).toBeNull();
  });

  it("skips the skeleton when loading ends within 150ms", () => {
    const { rerender } = render(<Gate queries={[q(true)]} />);
    tick(100);
    rerender(<Gate queries={[q(false)]} />);
    expect(screen.getByText("content")).toBeTruthy();
    tick(1000);
    expect(screen.queryByText("skeleton")).toBeNull();
  });

  it("keeps the skeleton for at least 300ms once shown", () => {
    const { rerender } = render(<Gate queries={[q(true)]} />);
    tick(150);
    tick(100);
    rerender(<Gate queries={[q(false)]} />);
    expect(screen.getByText("skeleton")).toBeTruthy();
    tick(199);
    expect(screen.getByText("skeleton")).toBeTruthy();
    tick(1);
    expect(screen.getByText("content")).toBeTruthy();
    expect(screen.queryByText("skeleton")).toBeNull();
  });

  it("switches at once when the skeleton has already been up for 300ms", () => {
    const { rerender } = render(<Gate queries={[q(true)]} />);
    tick(150 + 400);
    rerender(<Gate queries={[q(false)]} />);
    tick(0);
    expect(screen.getByText("content")).toBeTruthy();
  });

  it("lifts the gate when a query has failed (isPending is false)", () => {
    render(<Gate queries={[{ isPending: false, isError: true } as never]} />);
    expect(screen.getByText("content")).toBeTruthy();
  });

  it("waits for every query", () => {
    const { rerender } = render(<Gate queries={[q(true), q(true)]} />);
    tick(150);
    rerender(<Gate queries={[q(false), q(true)]} />);
    tick(1000);
    expect(screen.getByText("skeleton")).toBeTruthy();
  });

  it("accepts a render function as children", () => {
    render(
      <ReadyGate queries={[]} skeleton={null}>
        {() => <p>lazy</p>}
      </ReadyGate>,
    );
    expect(screen.getByText("lazy")).toBeTruthy();
  });
});
