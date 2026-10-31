import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { deleteScreenshot, importScreenshots, listScreenshots } from "../api/commands";
import { type DragDropHandlers, onFilesDropped } from "../api/dragDrop";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { Screenshot } from "../bindings/Screenshot";
import { ToastRegion } from "../components/toast/ToastRegion";
import { clearToasts } from "../components/toast/useToast";
import i18n from "../i18n";
import { ScreenshotsTab } from "./ScreenshotsTab";

vi.mock("../api/commands", () => ({
  listScreenshots: vi.fn(),
  deleteScreenshot: vi.fn(),
  importScreenshots: vi.fn(),
}));
vi.mock("../api/dragDrop", () => ({ onFilesDropped: vi.fn() }));
vi.mock("../api/assets", () => ({ fileUrl: (path: string) => `asset://${path}` }));

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

const shot = (name: string, modifiedMs: number): Screenshot => ({
  name, path: `C:/shots/${name}`, modifiedMs, size: 1,
});
const three = [shot("c.png", 3000), shot("b.png", 2000), shot("a.png", 1000)];

let drop: DragDropHandlers | undefined;

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ScreenshotsTab instance={instance} />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

const thumbs = () => within(screen.getByRole("list", { name: "Screenshots" })).getAllByRole("button");

beforeEach(async () => {
  await i18n.changeLanguage("en");
  drop = undefined;
  vi.mocked(listScreenshots).mockResolvedValue(three);
  vi.mocked(onFilesDropped).mockImplementation(async (h) => {
    drop = h;
    return () => {};
  });
});

afterEach(() => {
  cleanup();
  clearToasts();
  vi.resetAllMocks();
});

describe("ScreenshotsTab", () => {
  it("lists the screenshots in the order given, as lazy thumbnails", async () => {
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    expect(thumbs().map((b) => b.getAttribute("aria-label"))).toEqual(["Open c", "Open b", "Open a"]);
    const img = thumbs()[0]?.querySelector("img");
    expect(img?.getAttribute("src")).toBe("asset://C:/shots/c.png");
    expect(img?.getAttribute("loading")).toBe("lazy");
    expect(listScreenshots).toHaveBeenCalledWith("i1");
  });

  it("shows an empty state when there are none", async () => {
    vi.mocked(listScreenshots).mockResolvedValue([]);
    mount();
    expect(await screen.findByText("No screenshots yet")).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Screenshots" })).toBeNull();
  });

  it("opens a screenshot large and steps with the buttons and arrow keys", async () => {
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    fireEvent.click(thumbs()[1] as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "Screenshot" });
    expect(within(dialog).getByRole("img").getAttribute("src")).toBe("asset://C:/shots/b.png");

    fireEvent.click(within(dialog).getByRole("button", { name: "Next image" }));
    expect(within(dialog).getByRole("img").getAttribute("src")).toBe("asset://C:/shots/a.png");
    expect((within(dialog).getByRole("button", { name: "Next image" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.keyDown(within(dialog).getByRole("img"), { key: "ArrowLeft" });
    expect(within(dialog).getByRole("img").getAttribute("src")).toBe("asset://C:/shots/b.png");
    fireEvent.keyDown(within(dialog).getByRole("img"), { key: "ArrowLeft" });
    expect((within(dialog).getByRole("button", { name: "Previous image" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("deletes after a confirmation and moves on to the next screenshot", async () => {
    vi.mocked(deleteScreenshot).mockResolvedValue(null);
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    fireEvent.click(thumbs()[1] as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "Screenshot" });

    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(deleteScreenshot).not.toHaveBeenCalled();
    const confirm = await screen.findByRole("alertdialog");
    expect(within(confirm).getByText("Delete b?")).toBeTruthy();

    vi.mocked(listScreenshots).mockResolvedValue([three[0] as Screenshot, three[2] as Screenshot]);
    fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(deleteScreenshot).toHaveBeenCalledWith("i1", "b.png"));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    const viewer = await screen.findByRole("dialog", { name: "Screenshot" });
    expect(within(viewer).getByRole("img").getAttribute("src")).toBe("asset://C:/shots/a.png");
    await waitFor(() =>
      expect(within(screen.getByRole("list", { name: "Screenshots", hidden: true })).getAllByRole("button", { hidden: true })).toHaveLength(2),
    );
  });

  it("keeps the screenshot when the confirmation is cancelled", async () => {
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    fireEvent.click(thumbs()[0] as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "Screenshot" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("alertdialog");
    fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(deleteScreenshot).not.toHaveBeenCalled();
    const viewer = await screen.findByRole("dialog", { name: "Screenshot" });
    expect(within(viewer).getByRole("img").getAttribute("src")).toBe("asset://C:/shots/c.png");
  });

  it("shows a toast when deleting fails", async () => {
    vi.mocked(deleteScreenshot).mockRejectedValue(new IpcError({ code: "screenshot.not_found", params: {}, detail: null }));
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    fireEvent.click(thumbs()[0] as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "Screenshot" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("alertdialog");
    fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("That screenshot was not found.")).toBeTruthy();
  });

  it("imports dropped files and reports the result", async () => {
    vi.mocked(importScreenshots).mockResolvedValue({ added: 2, skipped: 0 });
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    await waitFor(() => expect(drop).toBeDefined());
    act(() => drop?.enter());
    expect(screen.getByText("Drop here to add")).toBeTruthy();
    act(() => drop?.drop(["C:/in/x.png", "C:/in/y.jpg"]));
    expect(screen.queryByText("Drop here to add")).toBeNull();
    await waitFor(() => expect(importScreenshots).toHaveBeenCalledWith("i1", ["C:/in/x.png", "C:/in/y.jpg"]));
    expect((await screen.findAllByText("Added 2 screenshots")).length).toBeGreaterThan(0);
    // 取り込んだあとは一覧を取り直す
    await waitFor(() => expect(listScreenshots).toHaveBeenCalledTimes(2));
  });

  it("tells when nothing could be added, and when only some could", async () => {
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    await waitFor(() => expect(drop).toBeDefined());
    vi.mocked(importScreenshots).mockResolvedValueOnce({ added: 0, skipped: 1 });
    act(() => drop?.drop(["C:/in/a.txt"]));
    expect(await screen.findByText("No images to add")).toBeTruthy();
    vi.mocked(importScreenshots).mockResolvedValueOnce({ added: 1, skipped: 2 });
    act(() => drop?.drop(["C:/in/a.png", "C:/in/b.txt", "C:/in/c.txt"]));
    expect(await screen.findByText("Added 1. 2 weren't images or couldn't be read")).toBeTruthy();
  });

  it("ignores drops while a screenshot is open", async () => {
    mount();
    await waitFor(() => expect(thumbs()).toHaveLength(3));
    await waitFor(() => expect(drop).toBeDefined());
    fireEvent.click(thumbs()[0] as HTMLElement);
    await screen.findByRole("dialog", { name: "Screenshot" });
    act(() => drop?.enter());
    expect(screen.queryByText("Drop here to add")).toBeNull();
    act(() => drop?.drop(["C:/in/x.png"]));
    expect(importScreenshots).not.toHaveBeenCalled();
  });

  it("releases the drop listener when it closes before registering finishes", async () => {
    const unlisten = vi.fn();
    let resolve: (fn: () => void) => void = () => {};
    vi.mocked(onFilesDropped).mockReturnValue(new Promise((r) => (resolve = r)));
    const view = mount();
    view.unmount();
    resolve(unlisten);
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });
});
