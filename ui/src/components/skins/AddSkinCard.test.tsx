import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { addSavedSkin, addSavedSkinFromPath } from "../../api/commands";
import { onFilesDropped, type DragDropHandlers } from "../../api/dragDrop";
import i18n from "../../i18n";
import { clearToasts, useToastList } from "../toast/useToast";
import { AddSkinCard } from "./AddSkinCard";

vi.mock("../../api/commands", () => ({ addSavedSkin: vi.fn(), addSavedSkinFromPath: vi.fn() }));
vi.mock("../../api/dragDrop", () => ({ onFilesDropped: vi.fn() }));

function Toasts() {
  const list = useToastList();
  return (
    <ul data-testid="toasts">
      {list.map((t) => (
        <li key={t.id}>{t.message}</li>
      ))}
    </ul>
  );
}

let handlers: DragDropHandlers;
const unlisten = vi.fn();

function mount(disabled = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const tree = (d: boolean) => (
    <QueryClientProvider client={client}>
      <AddSkinCard disabled={d} />
      <Toasts />
    </QueryClientProvider>
  );
  const view = render(tree(disabled));
  return { rerender: (d: boolean) => view.rerender(tree(d)), unmount: view.unmount };
}

const button = () => screen.getByRole("button", { name: "Add skin" }) as HTMLButtonElement;
const input = () => document.querySelector("input[type=file]") as HTMLInputElement;
const toasts = () => screen.getByTestId("toasts").querySelectorAll("li");
const file = (name: string, bytes: number[]) => new File([new Uint8Array(bytes)], name, { type: "image/png" });

async function drop(paths: string[]) {
  await act(async () => handlers.drop(paths));
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  clearToasts();
  vi.mocked(onFilesDropped).mockImplementation(async (h) => {
    handlers = h;
    return unlisten;
  });
  vi.mocked(addSavedSkin).mockResolvedValue({} as never);
  vi.mocked(addSavedSkinFromPath).mockResolvedValue({} as never);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  unlisten.mockReset();
});

describe("AddSkinCard", () => {
  it("shows the single phrase and no second line", async () => {
    mount();
    expect(button().textContent).toBe("Add skin");
    expect(screen.queryByText(/drag/i)).toBeNull();
    await i18n.changeLanguage("ja");
    await waitFor(() => expect(screen.getByRole("button", { name: "スキンを追加" })).toBeTruthy());
  });

  it("adds every dropped path in order and reports only the failure", async () => {
    vi.mocked(addSavedSkinFromPath).mockImplementation(async (p) => {
      if (p === "b.txt") throw { code: "skin.invalid", params: {}, detail: "x" };
      return {} as never;
    });
    mount();
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalledTimes(1));
    await drop(["a.png", "b.txt", "c.png"]);
    await waitFor(() => expect(addSavedSkinFromPath).toHaveBeenCalledTimes(3));
    expect(vi.mocked(addSavedSkinFromPath).mock.calls.map((c) => c[0])).toEqual(["a.png", "b.txt", "c.png"]);
    expect(toasts()).toHaveLength(1);
  });

  it("processes drops one at a time", async () => {
    let running = 0;
    let max = 0;
    vi.mocked(addSavedSkinFromPath).mockImplementation(async () => {
      max = Math.max(max, ++running);
      await new Promise((r) => setTimeout(r, 5));
      running--;
      return {} as never;
    });
    mount();
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalled());
    await drop(["a.png", "b.png"]);
    await waitFor(() => expect(addSavedSkinFromPath).toHaveBeenCalledTimes(2));
    expect(max).toBe(1);
  });

  it("opens the picker, adds each file by name without extension, and a failure does not stop the rest", async () => {
    vi.mocked(addSavedSkin).mockImplementation(async (name) => {
      if (name === "bad") throw { code: "skin.invalid", params: {}, detail: "x" };
      return {} as never;
    });
    mount();
    const click = vi.spyOn(input(), "click");
    fireEvent.click(button());
    expect(click).toHaveBeenCalled();
    expect(input().accept).toBe("image/png");
    expect(input().multiple).toBe(true);
    await act(async () => {
      fireEvent.change(input(), { target: { files: [file("bad.png", [1]), file("My Skin.v2.png", [7, 8, 9])] } });
    });
    await waitFor(() => expect(addSavedSkin).toHaveBeenCalledTimes(2));
    expect(vi.mocked(addSavedSkin).mock.calls).toEqual([
      ["bad", [1]],
      ["My Skin.v2", [7, 8, 9]],
    ]);
    expect(toasts()).toHaveLength(1);
  });

  it("highlights while dragging and clears on leave and drop", async () => {
    mount();
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalled());
    expect(button().dataset.dropTarget).toBeUndefined();
    act(() => handlers.enter());
    expect(button().dataset.dropTarget).toBe("true");
    act(() => handlers.leave());
    expect(button().dataset.dropTarget).toBeUndefined();
    act(() => handlers.enter());
    await drop([]);
    expect(button().dataset.dropTarget).toBeUndefined();
  });

  it("registers once, unregisters on unmount, and unlistens a late registration at once", async () => {
    const view = mount();
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalledTimes(1));
    view.rerender(false);
    expect(onFilesDropped).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(unlisten).not.toHaveBeenCalled());
    view.unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));

    let resolve!: (fn: () => void) => void;
    const late = vi.fn();
    vi.mocked(onFilesDropped).mockImplementation(() => new Promise((r) => (resolve = r)));
    mount().unmount();
    await act(async () => resolve(late));
    expect(late).toHaveBeenCalledTimes(1);
  });

  it("ignores drops and presses while disabled and keeps the same classes", async () => {
    const view = mount(false);
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalled());
    const idle = button().className;
    view.rerender(true);
    expect(button().disabled).toBe(true);
    const click = vi.spyOn(input(), "click");
    fireEvent.click(button());
    expect(click).not.toHaveBeenCalled();
    act(() => handlers.enter());
    expect(button().dataset.dropTarget).toBeUndefined();
    await drop(["a.png"]);
    expect(addSavedSkinFromPath).not.toHaveBeenCalled();
    // 大きさに関わるクラスは状態で変わらない（色だけが変わる）
    const size = (c: string) => c.split(" ").filter((x) => /^(p|m|w|h|aspect|border(-\d)?$|inset|gap)/.test(x));
    expect(size(button().className)).toEqual(size(idle));
  });
});
