import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { addSavedSkin, addSavedSkinFromPath, updateSavedSkin } from "../../api/commands";
import { onFilesDropped, type DragDropHandlers } from "../../api/dragDrop";
import i18n from "../../i18n";
import { clearToasts, useToastList } from "../toast/useToast";
import { useSkinActions } from "../../features/skins/useSkins";
import { AddSkinCard } from "./AddSkinCard";

function Card({ disabled }: { disabled: boolean }) {
  const actions = useSkinActions(null);
  return <AddSkinCard disabled={disabled} actions={actions} onAdding={() => {}} />;
}

vi.mock("../../api/commands", () => ({ addSavedSkin: vi.fn(), addSavedSkinFromPath: vi.fn(), updateSavedSkin: vi.fn() }));
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
      <Card disabled={d} />
      <Toasts />
    </QueryClientProvider>
  );
  const view = render(tree(disabled));
  return { rerender: (d: boolean) => view.rerender(tree(d)), unmount: view.unmount };
}

const button = () => screen.getByRole("button", { name: "Add skin" }) as HTMLButtonElement;
const input = () => document.querySelector("input[type=file]") as HTMLInputElement;
const nameInput = () => screen.getByRole("textbox", { name: "Name" }) as HTMLInputElement;
const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const saveButton = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
const openDialog = async () => {
  fireEvent.click(button());
  await screen.findByRole("dialog");
};
const choose = async (f: File) => {
  await act(async () => {
    fireEvent.change(input(), { target: { files: [f] } });
  });
};
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
  vi.mocked(addSavedSkin).mockResolvedValue({ id: "new", variant: "classic" } as never);
  vi.mocked(updateSavedSkin).mockResolvedValue({} as never);
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

  it("opens a dialog and sends nothing until Save", async () => {
    mount();
    await openDialog();
    expect(input().accept).toBe("image/png");
    expect(saveButton().disabled).toBe(true);
    await choose(file("My Skin.v2.png", [7, 8, 9]));
    expect(nameInput().value).toBe("My Skin.v2");
    expect(saveButton().disabled).toBe(false);
    expect(addSavedSkin).not.toHaveBeenCalled();
  });

  it("adds the chosen file with the edited name once, and closes", async () => {
    mount();
    await openDialog();
    await choose(file("a.png", [1, 2]));
    fireEvent.change(nameInput(), { target: { value: "  Mine  " } });
    // 名前を触ったあとにファイルを選び直しても、名前は上書きしない
    await choose(file("b.png", [3]));
    expect(nameInput().value).toBe("  Mine  ");
    fireEvent.click(saveButton());
    await waitFor(() => expect(addSavedSkin).toHaveBeenCalledTimes(1));
    expect(addSavedSkin).toHaveBeenCalledWith("Mine", [3]);
    expect(updateSavedSkin).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("sets the model afterwards when it differs from the one the skin was added with", async () => {
    mount();
    await openDialog();
    await choose(file("a.png", [1]));
    fireEvent.click(radio("Slim"));
    fireEvent.click(saveButton());
    await waitFor(() => expect(updateSavedSkin).toHaveBeenCalledWith("new", { name: null, variant: "slim", png: null }));
  });

  it("stays open and reports once when adding fails", async () => {
    vi.mocked(addSavedSkin).mockRejectedValue({ code: "skin.invalid", params: {}, detail: "x" });
    mount();
    await openDialog();
    await choose(file("a.png", [1]));
    fireEvent.click(saveButton());
    await waitFor(() => expect(toasts()).toHaveLength(1));
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("starts empty each time it is opened", async () => {
    mount();
    await openDialog();
    await choose(file("a.png", [1]));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await openDialog();
    expect(nameInput().value).toBe("");
    expect(saveButton().disabled).toBe(true);
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
    fireEvent.click(button());
    expect(screen.queryByRole("dialog")).toBeNull();
    act(() => handlers.enter());
    expect(button().dataset.dropTarget).toBeUndefined();
    await drop(["a.png"]);
    expect(addSavedSkinFromPath).not.toHaveBeenCalled();
    // 大きさに関わるクラスは状態で変わらない（色だけが変わる）
    const size = (c: string) => c.split(" ").filter((x) => /^(p|m|w|h|aspect|border(-\d)?$|inset|gap)/.test(x));
    expect(size(button().className)).toEqual(size(idle));
  });

  it("rejects an oversized picked file before reading it, with one toast", async () => {
    mount();
    await openDialog();
    const big = file("big.png", [1]);
    Object.defineProperty(big, "size", { value: 70 * 1024 });
    const read = vi.spyOn(big, "arrayBuffer");
    await choose(big);
    await waitFor(() => expect(toasts()).toHaveLength(1));
    expect(read).not.toHaveBeenCalled();
    expect(saveButton().disabled).toBe(true);
  });

  it("drops the highlight when the card becomes disabled mid-drag", async () => {
    const view = mount(false);
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalled());
    act(() => handlers.enter());
    expect(button().dataset.dropTarget).toBe("true");
    view.rerender(true);
    expect(button().dataset.dropTarget).toBeUndefined();
  });

  it("tells the parent about the whole batch, gaps between files included", async () => {
    const seen: boolean[] = [];
    let release!: () => void;
    vi.mocked(addSavedSkinFromPath).mockImplementationOnce(() => new Promise((r) => (release = () => r({} as never))));
    function Probe() {
      const actions = useSkinActions(null);
      return <AddSkinCard disabled={false} actions={actions} onAdding={(a) => seen.push(a)} />;
    }
    const client = new QueryClient();
    render(<QueryClientProvider client={client}><Probe /></QueryClientProvider>);
    await waitFor(() => expect(onFilesDropped).toHaveBeenCalled());
    await drop(["a.png", "b.png"]);
    expect(seen).toEqual([true]);
    await act(async () => release());
    await waitFor(() => expect(seen).toEqual([true, false]));
    expect(addSavedSkinFromPath).toHaveBeenCalledTimes(2);
  });
});
