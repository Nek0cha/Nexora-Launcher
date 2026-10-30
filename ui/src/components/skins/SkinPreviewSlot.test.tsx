import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// lazy は一度失敗すると失敗のままなので、テストごとにモジュールを読み直して新しい lazy を使う
async function load(factory: () => unknown) {
  vi.resetModules();
  vi.doMock("./SkinPreview", factory as never);
  return (await import("./SkinPreviewSlot")).SkinPreviewSlot;
}

const props = { image: null, variant: "classic" as const, capeImage: null };
let warn: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  // React は境界が捕まえた失敗も console.error に出す
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.doUnmock("./SkinPreview");
  vi.restoreAllMocks();
});

describe("SkinPreviewSlot", () => {
  it("keeps the empty frame, with no text, when the chunk fails to load, and warns once", async () => {
    const Slot = await load(() => {
      throw new Error("chunk 404");
    });
    const { rerender } = render(<Slot {...props} />);
    await waitFor(() => expect(warn).toHaveBeenCalledTimes(1));
    expect(warn.mock.calls[0]?.[0]).toBe("SkinPreview: chunk load failed");
    const slot = screen.getByTestId("skin-preview-slot");
    expect(slot.textContent).toBe("");
    expect(slot.querySelector("[role=alert], button")).toBeNull();
    // 再描画しても同じ枠のまま、再試行も警告の繰り返しもしない
    rerender(<Slot {...props} image="data:image/png;base64,AA" />);
    expect(screen.getByTestId("skin-preview-slot")).toBe(slot);
    expect(warn).toHaveBeenCalledTimes(1);
  });

  it("shows a same-size empty fallback in the same slot while loading, then the preview replaces it", async () => {
    let resolve!: (m: unknown) => void;
    const ready = new Promise((r) => (resolve = r));
    const Slot = await load(async () => ready as never);
    render(<Slot {...props} />);
    const slot = screen.getByTestId("skin-preview-slot");
    const fallback = screen.getByTestId("skin-preview-fallback");
    expect(slot.contains(fallback)).toBe(true);
    expect(fallback.className).toContain("size-full");
    expect(slot.className).toContain("h-96");
    expect(slot.className).toContain("w-64");
    expect(fallback.textContent).toBe("");

    resolve({ SkinPreview: () => <div data-testid="preview" /> });
    await waitFor(() => expect(screen.getByTestId("preview")).toBeTruthy());
    expect(screen.queryByTestId("skin-preview-fallback")).toBeNull();
    expect(screen.getByTestId("skin-preview-slot")).toBe(slot);
    expect(warn).not.toHaveBeenCalled();
  });
});
