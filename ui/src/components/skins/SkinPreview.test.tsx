import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { SkinPreview } from "./SkinPreview";

// jsdom には WebGL が無いので、ライブラリは差し替える。ここでは「props がライブラリへ届くこと」と「寿命」を確かめる。
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

function viewer() {
  const v = mock.FakeViewer.instances[0];
  if (!v) throw new Error("viewer was not created");
  return v;
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

  it("loads the skin with the default model for classic and the slim model for slim", () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(1);
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN, { model: "default" });
    rerender(<SkinPreview image={SKIN} variant="slim" capeImage={null} />);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(2);
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN, { model: "slim" });
  });

  it("reloads when the image changes and keeps the same viewer", () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="slim" capeImage={null} />);
    rerender(<SkinPreview image={SKIN_B} variant="slim" capeImage={null} />);
    expect(mock.FakeViewer.instances).toHaveLength(1);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(2);
    expect(viewer().loadSkin).toHaveBeenLastCalledWith(SKIN_B, { model: "slim" });
    expect(viewer().dispose).not.toHaveBeenCalled();
  });

  it("does not reload anything when re-rendered with the same props", () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    rerender(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(1);
    expect(viewer().loadCape).toHaveBeenCalledTimes(1);
  });

  it("does not throw or load a null skin when the account has no skin", () => {
    expect(() => render(<SkinPreview image={null} variant="classic" capeImage={null} />)).not.toThrow();
    expect(viewer().loadSkin).not.toHaveBeenCalled();
    expect(viewer().resetSkin).toHaveBeenCalled();
  });

  it("clears the model when the image becomes null", () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    viewer().resetSkin.mockClear();
    rerender(<SkinPreview image={null} variant="classic" capeImage={null} />);
    expect(viewer().resetSkin).toHaveBeenCalledTimes(1);
    expect(viewer().loadSkin).toHaveBeenCalledTimes(1);
  });

  it("shows the cape when given one and removes it when cleared", () => {
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={CAPE} />);
    expect(viewer().loadCape).toHaveBeenCalledWith(CAPE);
    rerender(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
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
    mock.FakeViewer.failOnCreate = true;
    const { container } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    expect(container.firstElementChild).not.toBeNull();
    expect(container.querySelector("canvas")).toBeNull();
  });

  it("swallows a texture that fails to load", async () => {
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    const { rerender } = render(<SkinPreview image={SKIN} variant="classic" capeImage={null} />);
    viewer().loadSkin.mockRejectedValueOnce(new Error("bad texture"));
    viewer().loadCape.mockRejectedValueOnce(new Error("bad texture"));
    rerender(<SkinPreview image={SKIN_B} variant="classic" capeImage={CAPE} />);
    await new Promise((resolve) => setTimeout(resolve, 0));
    process.off("unhandledRejection", unhandled);
    expect(unhandled).not.toHaveBeenCalled();
  });
});
