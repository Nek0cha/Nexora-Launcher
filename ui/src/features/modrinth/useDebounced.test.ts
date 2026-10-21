import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDebouncedValue } from "./useDebounced";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("useDebouncedValue", () => {
  it("emits only the last value after the delay", () => {
    const { result, rerender } = renderHook(({ v }) => useDebouncedValue(v, 300), { initialProps: { v: "a" } });
    rerender({ v: "b" });
    act(() => void vi.advanceTimersByTime(100));
    rerender({ v: "c" });
    act(() => void vi.advanceTimersByTime(100));
    rerender({ v: "d" });
    expect(result.current).toBe("a");
    act(() => void vi.advanceTimersByTime(299));
    expect(result.current).toBe("a");
    act(() => void vi.advanceTimersByTime(1));
    expect(result.current).toBe("d");
  });
});
