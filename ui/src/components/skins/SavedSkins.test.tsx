import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  addSavedSkinFromPath,
  applySavedSkin,
  getCurrentSkin,
  getSavedSkinImage,
  importCurrentSkin,
  listSavedSkins,
} from "../../api/commands";
import type { CurrentSkin } from "../../bindings/CurrentSkin";
import type { SavedSkin } from "../../bindings/SavedSkin";
import { onFilesDropped, type DragDropHandlers } from "../../api/dragDrop";
import i18n from "../../i18n";
import { clearToasts, useToastList } from "../toast/useToast";
import { SavedSkins } from "./SavedSkins";

vi.mock("../../api/dragDrop", () => ({ onFilesDropped: vi.fn() }));
vi.mock("../../api/commands", () => ({
  addSavedSkinFromPath: vi.fn(),
  applySavedSkin: vi.fn(),
  getCurrentSkin: vi.fn(),
  getSavedSkinImage: vi.fn(),
  importCurrentSkin: vi.fn(),
  listSavedSkins: vi.fn(),
}));

const skin = (id: string, patch: Partial<SavedSkin> = {}): SavedSkin => ({
  id, name: `Skin ${id}`, variant: "classic", sha256: `h-${id}`, addedAtMs: 1, ...patch,
});
const PNG = "data:image/png;base64,AAAA";
const current = (sha256: string | null, image: string | null = sha256 ? PNG : null): CurrentSkin => ({
  image, sha256, variant: "classic", capes: [],
});

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

type Props = { accountId: string | null; canApply?: boolean };

function mount(props: Props = { accountId: "a1" }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const tree = (p: Props) => (
    <QueryClientProvider client={client}>
      <SavedSkins accountId={p.accountId} canApply={p.canApply ?? true} />
      <Toasts />
    </QueryClientProvider>
  );
  const view = render(tree(props));
  return { rerender: (p: Props) => view.rerender(tree(p)) };
}

const card = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;
// 進行中の印（aria-busy）はカードの枠に付く
const busy = (name: string) => card(name).parentElement?.getAttribute("aria-busy") ?? null;
// チェックが付いているカード（aria-pressed が真）の名前
const checkedNames = () =>
  screen
    .queryAllByRole("button")
    .filter((b) => b.getAttribute("aria-pressed") === "true")
    .map((b) => b.getAttribute("aria-label"));

beforeEach(async () => {
  await i18n.changeLanguage("en");
  clearToasts();
  vi.mocked(onFilesDropped).mockResolvedValue(() => {});
  vi.mocked(listSavedSkins).mockResolvedValue([skin("1"), skin("2"), skin("3")]);
  vi.mocked(getSavedSkinImage).mockResolvedValue(PNG);
  vi.mocked(getCurrentSkin).mockResolvedValue(current("h-1"));
  vi.mocked(importCurrentSkin).mockResolvedValue(null);
  vi.mocked(applySavedSkin).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("SavedSkins", () => {
  it("disables every skin card for the whole add batch and applies nothing", async () => {
    let handlers!: DragDropHandlers;
    vi.mocked(onFilesDropped).mockImplementation(async (h) => {
      handlers = h;
      return () => {};
    });
    let release!: () => void;
    vi.mocked(addSavedSkinFromPath).mockImplementation(() => new Promise((r) => (release = () => r({} as never))));
    mount();
    await screen.findByRole("button", { name: "Skin 2" });
    await waitFor(() => expect(handlers).toBeTruthy());
    await act(async () => handlers.drop(["a.png"]));
    for (const n of ["Skin 1", "Skin 2", "Skin 3"]) expect(card(n).disabled).toBe(true);
    fireEvent.click(card("Skin 2"));
    expect(applySavedSkin).not.toHaveBeenCalled();
    await act(async () => release());
    await waitFor(() => expect(card("Skin 2").disabled).toBe(false));
  });

  it("puts the add card first, and keeps it usable when skins cannot be applied", async () => {
    mount({ accountId: "a1", canApply: false });
    await screen.findByRole("button", { name: "Skin 1" });
    const buttons = screen.getAllByRole("button") as HTMLButtonElement[];
    expect(buttons[0]?.textContent).toBe("Add skin");
    expect(buttons[0]?.disabled).toBe(false);
    expect(card("Skin 1").disabled).toBe(true);
  });

  it("renders one card per saved skin, named by the skin", async () => {
    mount();
    expect(await screen.findByRole("button", { name: "Skin 1" })).toBeTruthy();
    expect(screen.getAllByRole("button")).toHaveLength(4); // 追加のカード + スキン 3 枚
  });

  it("marks only the card whose hash matches the current skin", async () => {
    mount();
    await waitFor(() => expect(checkedNames()).toEqual(["Skin 1"]));
    expect(card("Skin 2").getAttribute("aria-pressed")).toBe("false");
  });

  it("marks nothing when no saved skin matches", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(current("other"));
    mount();
    await screen.findByRole("button", { name: "Skin 1" });
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalled());
    expect(checkedNames()).toEqual([]);
  });

  it("applies the pressed skin to the account", async () => {
    mount();
    await screen.findByRole("button", { name: "Skin 2" });
    fireEvent.click(card("Skin 2"));
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledWith("a1", "2"));
  });

  it("does not apply the skin that is already worn", async () => {
    mount();
    await waitFor(() => expect(checkedNames()).toEqual(["Skin 1"]));
    fireEvent.click(card("Skin 1"));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });

  it("marks the pressed card busy and disables every card while applying", async () => {
    let finishApply!: () => void;
    vi.mocked(applySavedSkin).mockReturnValue(new Promise((r) => (finishApply = () => r(null))));
    mount();
    await screen.findByRole("button", { name: "Skin 2" });
    fireEvent.click(card("Skin 2"));
    await waitFor(() => expect(busy("Skin 2")).toBe("true"));
    expect(busy("Skin 1")).toBeNull();
    for (const name of ["Skin 1", "Skin 2", "Skin 3"]) expect(card(name).disabled).toBe(true);
    fireEvent.click(card("Skin 3"));
    expect(applySavedSkin).toHaveBeenCalledTimes(1);
    await act(async () => finishApply());
  });

  it("keeps the check on the old card until the refetch finishes, then moves it once", async () => {
    let finishRefetch!: (v: CurrentSkin) => void;
    let reads = 0;
    vi.mocked(getCurrentSkin).mockImplementation(() => {
      reads += 1;
      return reads === 1 ? Promise.resolve(current("h-1")) : new Promise((r) => (finishRefetch = r));
    });
    mount();
    await waitFor(() => expect(checkedNames()).toEqual(["Skin 1"]));

    // チェックの履歴。変化のたびに、チェックが付いているカードを記録する
    const history: string[] = [];
    const record = () => history.push(checkedNames().join(","));
    const observer = new MutationObserver(record);
    observer.observe(document.body, {
      attributes: true,
      subtree: true,
      childList: true,
      attributeFilter: ["aria-pressed", "aria-busy", "disabled"],
    });
    record();

    fireEvent.click(card("Skin 2"));
    await waitFor(() => expect(reads).toBe(2)); // 適用が済み、取り直しが始まった
    expect(busy("Skin 2")).toBe("true"); // まだ進行中
    expect(checkedNames()).toEqual(["Skin 1"]);

    await act(async () => finishRefetch(current("h-2")));
    await waitFor(() => expect(checkedNames()).toEqual(["Skin 2"]));
    await waitFor(() => expect(busy("Skin 2")).toBeNull());
    observer.disconnect();

    // 履歴は「Skin 1」から「Skin 2」へ 1 回だけ。空の間も、Skin 1 に戻ることもない
    const changes = history.filter((h, i) => i === 0 || h !== history[i - 1]);
    expect(changes).toEqual(["Skin 1", "Skin 2"]);
    expect(card("Skin 3").disabled).toBe(false);
  });

  it("shows an error toast when the apply fails and leaves the check where it was", async () => {
    vi.mocked(applySavedSkin).mockRejectedValue({ code: "app.unexpected", params: {}, detail: "401" });
    mount();
    await waitFor(() => expect(checkedNames()).toEqual(["Skin 1"]));
    fireEvent.click(card("Skin 2"));
    await waitFor(() => expect(screen.getByTestId("toasts").textContent).toContain("An unexpected problem occurred"));
    expect(checkedNames()).toEqual(["Skin 1"]);
    await waitFor(() => expect(card("Skin 3").disabled).toBe(false));
    expect(busy("Skin 2")).toBeNull();
  });

  it("renders disabled cards, with no mark and no request for the current skin, when the account cannot apply", async () => {
    mount({ accountId: "a1", canApply: false });
    await screen.findByRole("button", { name: "Skin 1" });
    for (const name of ["Skin 1", "Skin 2", "Skin 3"]) expect(card(name).disabled).toBe(true);
    await act(async () => {});
    expect(getCurrentSkin).not.toHaveBeenCalled();
    expect(checkedNames()).toEqual([]);
    fireEvent.click(card("Skin 2"));
    expect(applySavedSkin).not.toHaveBeenCalled();
  });
});

describe("import of the current skin", () => {
  it("imports once per account, and refreshes the list when a skin was added", async () => {
    vi.mocked(importCurrentSkin).mockResolvedValueOnce(skin("9", { sha256: "h-9" }));
    const { rerender } = mount();
    await waitFor(() => expect(importCurrentSkin).toHaveBeenCalledWith("a1"));
    await waitFor(() => expect(vi.mocked(listSavedSkins).mock.calls.length).toBeGreaterThanOrEqual(2));
    rerender({ accountId: "a1" });
    rerender({ accountId: "a1", canApply: false });
    rerender({ accountId: "a1" });
    await act(async () => {});
    expect(importCurrentSkin).toHaveBeenCalledTimes(1);
  });

  it("does not refetch the list when nothing was imported", async () => {
    mount();
    await waitFor(() => expect(importCurrentSkin).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(listSavedSkins).toHaveBeenCalledTimes(1);
  });

  it("does not import again after the apply refetches the current skin", async () => {
    mount();
    await screen.findByRole("button", { name: "Skin 2" });
    await waitFor(() => expect(importCurrentSkin).toHaveBeenCalledTimes(1));
    vi.mocked(getCurrentSkin).mockResolvedValue(current("h-2"));
    fireEvent.click(card("Skin 2"));
    await waitFor(() => expect(checkedNames()).toEqual(["Skin 2"]));
    await act(async () => {});
    expect(importCurrentSkin).toHaveBeenCalledTimes(1);
  });

  it("imports again for another account", async () => {
    const { rerender } = mount();
    await waitFor(() => expect(importCurrentSkin).toHaveBeenCalledWith("a1"));
    rerender({ accountId: "a2" });
    await waitFor(() => expect(importCurrentSkin).toHaveBeenCalledWith("a2"));
    expect(importCurrentSkin).toHaveBeenCalledTimes(2);
  });

  it("does not import without an account", async () => {
    mount({ accountId: null });
    await screen.findByRole("button", { name: "Skin 1" });
    await act(async () => {});
    expect(importCurrentSkin).not.toHaveBeenCalled();
    expect(getCurrentSkin).not.toHaveBeenCalled();
  });

  it("does not import when the account cannot apply", async () => {
    mount({ accountId: "a1", canApply: false });
    await screen.findByRole("button", { name: "Skin 1" });
    await act(async () => {});
    expect(getCurrentSkin).not.toHaveBeenCalled();
    expect(importCurrentSkin).not.toHaveBeenCalled();
  });

  it("does not import when the current skin has no image", async () => {
    vi.mocked(getCurrentSkin).mockResolvedValue(current(null));
    mount();
    await screen.findByRole("button", { name: "Skin 1" });
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalled());
    await act(async () => {});
    expect(importCurrentSkin).not.toHaveBeenCalled();
  });

  it("stays silent when the import fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.mocked(importCurrentSkin).mockRejectedValue({ code: "app.unexpected", params: {}, detail: "x" });
    mount();
    await waitFor(() => expect(warn).toHaveBeenCalled());
    expect(screen.getByTestId("toasts").textContent).toBe("");
    expect(screen.getAllByRole("button")).toHaveLength(4); // 追加のカード + スキン 3 枚
    warn.mockRestore();
  });
});
