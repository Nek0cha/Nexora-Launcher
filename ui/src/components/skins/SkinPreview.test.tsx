import { act, cleanup, render } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import i18n from "../../i18n";
import { SkinPreview } from "./SkinPreview";

// jsdom には WebGL が無いので、ライブラリは差し替える。ここでは「props がライブラリへ届くこと」と「寿命」「読み込みの順序」を確かめる。
const mock = vi.hoisted(() => {
  class FakeViewer {
    static instances: FakeViewer[] = [];
    static failOnCreate = false;
    options: Record<string, unknown>;
    canvas = document.createElement("canvas");
    autoRotate = false;
    autoRotateSpeed = 1;
    renderPaused = false;
    controls = { enableZoom: true, enablePan: true };
    loadSkin = vi.fn((_source: unknown, _options?: unknown): Promise<void> => Promise.resolve());
    loadCape = vi.fn((_source: unknown): Promise<void> => Promise.resolve());
    resetSkin = vi.fn();
    resetCape = vi.fn();
    setSize = vi.fn();
    dispose = vi.fn();
    constructor(options: Record<string, unknown>) {
      if (FakeViewer.failOnCreate) throw new Error("WebGL is not available");
      this.options = options;
      this.renderPaused = options.renderPaused === true;
      FakeViewer.instances.push(this);
    }
  }
  return { FakeViewer };
});

vi.mock("skinview3d", () => ({ SkinViewer: mock.FakeViewer }));

const SKIN = "data:image/png;base64,AAAA";
const SKIN_B = "data:image/png;base64,BBBB";
const SKIN_C = "data:image/png;base64,DDDD";
const CAPE = "data:image/png;base64,CCCC";

// 動きを減らす設定。テストから切り替えて change を流す
let reduced = false;
let mediaListeners: Array<() => void> = [];
let resizeCallback: ResizeObserverCallback = () => {};
let resizeDisconnect = vi.fn();
let hidden = false;

beforeEach(() => {
  mock.FakeViewer.instances = [];
  mock.FakeViewer.failOnCreate = false;
  reduced = false;
  mediaListeners = [];
  hidden = false;
  resizeDisconnect = vi.fn();
  vi.stubGlobal("matchMedia", (query: string) => ({
    get matches() {
      return query.includes("prefers-reduced-motion") && reduced;
    },
    media: query,
    addEventListener: (_: string, fn: () => void) => mediaListeners.push(fn),
    removeEventListener: (_: string, fn: () => void) => {
      mediaListeners = mediaListeners.filter((l) => l !== fn);
    },
  }));
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: ResizeObserverCallback) {
        resizeCallback = cb;
      }
      observe() {}
      unobserve() {}
      disconnect = resizeDisconnect;
    },
  );
  Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  Reflect.deleteProperty(document, "hidden");
});

// 読み込みの直列化は microtask で進むので、反映を待つ
const flush = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

function viewer() {
  const v = mock.FakeViewer.instances[0];
  if (!v) throw new Error("viewer was not created");
  return v;
}

// 終わる時をテストが決められる読み込み。呼ばれた順に resolvers へ積む
function pendingLoads(fn: Mock<(source: unknown, options?: unknown) => Promise<void>>) {
  const resolvers: Array<() => void> = [];
  fn.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        resolvers.push(resolve);
      }),
  );
  return resolvers;
}

describe("SkinPreview", () => {
  it("creates one viewer and puts its canvas in the wrapper, hidden from assistive tech", async () => {
    await i18n.changeLanguage("en");
    const { container } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(mock.FakeViewer.instances).toHaveLength(1);
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.getAttribute("role")).toBe("img");
    expect(wrapper.getAttribute("aria-label")).toBe("Skin preview");
    expect(wrapper.textContent).toBe("");
    expect(viewer().canvas.parentElement).toBe(wrapper);
    expect(viewer().canvas.getAttribute("aria-hidden")).toBe("true");
    // ドラッグで回すだけにする。ホイールでの拡大や移動は止める
    expect(viewer().controls.enableZoom).toBe(false);
    expect(viewer().controls.enablePan).toBe(false);
  });

  it("loads the skin with the default model for classic and the slim model for slim", async () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(1);
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN, { model: "default" });
    rerender(<SkinPreview image={SKIN} variant="slim" capeImage={null} />);
    await flush();
    expect(viewer().loadSkin).toHaveBeenCalledTimes(2);
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN, { model: "slim" });
  });

  it("reloads when the image changes and keeps the same viewer", async () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="slim" capeImage={null} />);
    rerender(<SkinPreview image={SKIN_B} variant="slim" capeImage={null} />);
    await flush();
    expect(mock.FakeViewer.instances).toHaveLength(1);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(2);
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN_B, { model: "slim" });
    expect(viewer().dispose).not.toHaveBeenCalled();
  });

  it("does not reload anything when re-rendered with the same props", async () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    rerender(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    await flush();
    expect(viewer().loadSkin).toHaveBeenCalledTimes(1);
    expect(viewer().loadCape).toHaveBeenCalledTimes(1);
  });

  it("does not throw or load a null skin when the account has no skin", () => {
    expect(() => render(<SkinPreview image={null} variant="classic" capeImage={null} />)).not.toThrow();
    expect(viewer().loadSkin).not.toHaveBeenCalled();
    expect(viewer().resetSkin).toHaveBeenCalled();
  });

  it("clears the model when the image becomes null", async () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    await flush();
    viewer().resetSkin.mockClear();
    rerender(<SkinPreview image={null} variant="classic" capeImage={null} />);
    await flush();
    expect(viewer().resetSkin).toHaveBeenCalledTimes(1);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(1);
  });

  it("shows the cape when given one and removes it when cleared", async () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    expect(viewer().loadCape).toHaveBeenCalledWith(CAPE);
    rerender(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    await flush();
    expect(viewer().resetCape).toHaveBeenCalledTimes(1);
  });

  it("disposes the viewer and removes the canvas on unmount", () => {
    const { container, unmount } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    const v = viewer();
    const wrapper = container.firstElementChild as HTMLElement;
    unmount();
    expect(v.dispose).toHaveBeenCalledTimes(1);
    expect(wrapper.contains(v.canvas)).toBe(false);
    expect(resizeDisconnect).toHaveBeenCalled();
    expect(mediaListeners).toHaveLength(0);
  });

  it("auto-rotates normally and not under prefers-reduced-motion", () => {
    render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(viewer().autoRotate).toBe(true);
    cleanup();
    mock.FakeViewer.instances = [];
    reduced = true;
    render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(viewer().autoRotate).toBe(false);
  });

  it("follows a change of the reduced-motion setting without recreating the viewer", () => {
    render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(viewer().autoRotate).toBe(true);
    reduced = true;
    act(() => mediaListeners.forEach((l) => l()));
    expect(viewer().autoRotate).toBe(false);
    reduced = false;
    act(() => mediaListeners.forEach((l) => l()));
    expect(viewer().autoRotate).toBe(true);
    expect(mock.FakeViewer.instances).toHaveLength(1);
  });

  it("starts paused when the document is hidden, and follows visibility changes", () => {
    hidden = true;
    render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(viewer().options.renderPaused).toBe(true);
    expect(viewer().renderPaused).toBe(true);
    hidden = false;
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(viewer().renderPaused).toBe(false);
    hidden = true;
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(viewer().renderPaused).toBe(true);
  });

  it("stops listening to visibility changes after unmount", () => {
    const { unmount } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    const v = viewer();
    unmount();
    hidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
    expect(v.renderPaused).toBe(false);
  });

  it("sizes the canvas to its container and ignores an empty size", () => {
    render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    act(() =>
      resizeCallback([{ contentRect: { width: 320, height: 480 } } as ResizeObserverEntry], {} as ResizeObserver),
    );
    expect(viewer().setSize).toHaveBeenLastCalledWith(320, 480);
    viewer().setSize.mockClear();
    act(() =>
      resizeCallback([{ contentRect: { width: 0, height: 0 } } as ResizeObserverEntry], {} as ResizeObserver),
    );
    expect(viewer().setSize).not.toHaveBeenCalled();
  });

  it("renders an empty stage instead of throwing when WebGL is unavailable", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mock.FakeViewer.failOnCreate = true;
    const { container } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(container.firstElementChild).not.toBeNull();
    expect(container.querySelector("canvas")).toBeNull();
    expect(warn).toHaveBeenCalledWith("SkinPreview: viewer create failed", expect.any(Error));
    warn.mockRestore();
  });

  it("logs a texture that fails to load and keeps processing later requests", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const unhandled = vi.fn();
    // CI の型検査には node の型が無いので、必要な形だけを自前で宣言する
    const proc = (globalThis as unknown as { process: { on(e: string, f: () => void): void; off(e: string, f: () => void): void } }).process;
    proc.on("unhandledRejection", unhandled);
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    await flush();
    viewer().loadSkin.mockRejectedValueOnce(new Error("bad texture"));
    viewer().loadCape.mockRejectedValueOnce(new Error("bad texture"));
    rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={CAPE} />);
    await flush();
    rerender(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    await flush();
    proc.off("unhandledRejection", unhandled);
    expect(unhandled).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledWith("SkinPreview: skin load failed", expect.any(Error));
    expect(warn).toHaveBeenCalledWith("SkinPreview: cape load failed", expect.any(Error));
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN, { model: "default" });
    warn.mockRestore();
  });

  describe("ordering of loads", () => {
    it("starts a newer load only after the older one settles, so a slow old load cannot win", async () => {
      const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      const v = viewer();
      v.loadSkin.mockClear();
      const resolvers = pendingLoads(v.loadSkin);
      rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={null} />);
      rerender(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      // 古い読み込み（SKIN_B）が終わるまで、新しい要求（SKIN）は始まらない
      expect(v.loadSkin.mock.calls.map((c) => c[0])).toEqual([SKIN_B]);
      resolvers[0]?.();
      await flush();
      // 最後に反映されるのは、最新の要求
      expect(v.loadSkin.mock.calls.map((c) => c[0])).toEqual([SKIN_B, SKIN]);
      resolvers[1]?.();
      await flush();
      expect(v.loadSkin).toHaveBeenCalledTimes(2);
    });

    it("applies only the newest request after an in-flight load, skipping the ones in between", async () => {
      const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      const v = viewer();
      v.loadSkin.mockClear();
      const resolvers = pendingLoads(v.loadSkin);
      rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={null} />);
      await flush();
      expect(v.loadSkin).toHaveBeenCalledTimes(1);
      // 読み込み中に 2 回変わる。途中のものは捨てられ、最新だけが残る
      rerender(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      rerender(<SkinPreview image={SKIN_C} variant="slim" capeImage={null} />);
      await flush();
      expect(v.loadSkin).toHaveBeenCalledTimes(1);
      resolvers[0]?.();
      await flush();
      expect(v.loadSkin).toHaveBeenCalledTimes(2);
      expect(v.loadSkin).toHaveBeenLastCalledWith(SKIN_C, { model: "slim" });
    });

    it("applies a reset requested during a load after that load, so the reset is last", async () => {
      const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      const v = viewer();
      const order: string[] = [];
      const resolvers = pendingLoads(v.loadSkin);
      v.loadSkin.mockImplementationOnce(() => {
        order.push("load");
        return new Promise<void>((resolve) => {
          resolvers.push(() => {
            order.push("loaded");
            resolve();
          });
        });
      });
      v.resetSkin.mockImplementation(() => {
        order.push("reset");
      });
      rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={null} />);
      await flush();
      rerender(<SkinPreview image={null} variant="classic" capeImage={null} />);
      await flush();
      expect(order).toEqual(["load"]);
      resolvers[0]?.();
      await flush();
      expect(order).toEqual(["load", "loaded", "reset"]);
    });

    it("orders cape loads and resets the same way", async () => {
      const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      const v = viewer();
      const order: string[] = [];
      const resolvers: Array<() => void> = [];
      v.loadCape.mockImplementationOnce(() => {
        order.push("load");
        return new Promise<void>((resolve) => {
          resolvers.push(() => {
            order.push("loaded");
            resolve();
          });
        });
      });
      v.resetCape.mockImplementation(() => {
        order.push("reset");
      });
      rerender(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
      rerender(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      expect(order).toEqual(["load"]);
      resolvers[0]?.();
      await flush();
      expect(order).toEqual(["load", "loaded", "reset"]);
    });

    it("does not touch the viewer when a load settles after unmount", async () => {
      const { rerender, unmount } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      const v = viewer();
      v.loadSkin.mockClear();
      const resolvers = pendingLoads(v.loadSkin);
      rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={null} />);
      await flush();
      // 読み込み中に次の要求が溜まったまま、破棄される
      rerender(<SkinPreview image={SKIN_C} variant="slim" capeImage={null} />);
      unmount();
      resolvers[0]?.();
      await flush();
      expect(v.loadSkin).toHaveBeenCalledTimes(1);
      expect(v.resetSkin).not.toHaveBeenCalled();
      expect(v.dispose).toHaveBeenCalledTimes(1);
    });

    it("does not log or call anything after unmount even when the in-flight load rejects", async () => {
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      const { rerender, unmount } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
      await flush();
      const v = viewer();
      v.loadSkin.mockClear();
      let fail!: (e: unknown) => void;
      v.loadSkin.mockImplementationOnce(
        () =>
          new Promise<void>((_, reject) => {
            fail = reject;
          }),
      );
      rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={null} />);
      await flush();
      rerender(<SkinPreview image={SKIN_C} variant="classic" capeImage={null} />);
      unmount();
      fail(new Error("late"));
      await flush();
      expect(v.loadSkin).toHaveBeenCalledTimes(1);
      warn.mockRestore();
    });
  });

  describe("StrictMode", () => {
    it("leaves exactly one live viewer with every listener registered once", async () => {
      const added = vi.spyOn(document, "addEventListener");
      const removed = vi.spyOn(document, "removeEventListener");
      const { container, unmount } = render(
        <StrictMode>
          <SkinPreview image={SKIN} variant="slim" capeImage={CAPE} />
        </StrictMode>,
      );
      await flush();
      const live = mock.FakeViewer.instances.filter((v) => v.dispose.mock.calls.length === 0);
      expect(mock.FakeViewer.instances).toHaveLength(2);
      expect(live).toHaveLength(1);
      expect(mock.FakeViewer.instances[0]?.dispose).toHaveBeenCalledTimes(1);
      const wrapper = container.firstElementChild as HTMLElement;
      expect(wrapper.querySelectorAll("canvas")).toHaveLength(1);
      expect(wrapper.querySelector("canvas")).toBe(live[0]?.canvas);
      // 動きを減らす設定の購読は 1 つだけ残る
      expect(mediaListeners).toHaveLength(1);
      const visibility = (spy: typeof added) => spy.mock.calls.filter(([type]) => type === "visibilitychange").length;
      expect(visibility(added) - visibility(removed)).toBe(1);
      // 生きている方に、最新の props が反映される
      expect(live[0]?.loadSkin).toHaveBeenLastCalledWith(SKIN, { model: "slim" });
      expect(live[0]?.loadCape).toHaveBeenLastCalledWith(CAPE);
      unmount();
      expect(visibility(added) - visibility(removed)).toBe(0);
      expect(mediaListeners).toHaveLength(0);
      expect(mock.FakeViewer.instances.every((v) => v.dispose.mock.calls.length === 1)).toBe(true);
      added.mockRestore();
      removed.mockRestore();
    });
  });
});
