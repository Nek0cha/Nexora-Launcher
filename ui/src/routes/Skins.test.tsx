import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  addSavedSkinFromPath,
  applyDefaultSkin,
  applySavedSkin,
  getCurrentSkin,
  getSavedSkinImage,
  getSkin,
  importCurrentSkin,
  listAccounts,
  listDefaultSkins,
  listSavedSkins,
  setCape,
} from "../api/commands";
import { onFilesDropped, type DragDropHandlers } from "../api/dragDrop";
import { onAccountsChanged, onLoginProgress } from "../api/events";
import type { Account } from "../bindings/Account";
import type { AccountList } from "../bindings/AccountList";
import { ACCOUNTS_KEY } from "../features/accounts/useAccounts";
import i18n from "../i18n";
import { CHANGE_COOLDOWN_MS, resetCooldown } from "../features/skins/useSkins";
import Skins from "./Skins";

vi.mock("../api/commands", () => ({
  listAccounts: vi.fn(),
  listSavedSkins: vi.fn(),
  listDefaultSkins: vi.fn(),
  applyDefaultSkin: vi.fn(),
  getCurrentSkin: vi.fn(),
  getSavedSkinImage: vi.fn(),
  importCurrentSkin: vi.fn(),
  applySavedSkin: vi.fn(),
  updateSavedSkin: vi.fn(),
  addSavedSkinFromPath: vi.fn(),
  setCape: vi.fn(),
  beginLogin: vi.fn(),
  cancelTask: vi.fn(),
  getSkin: vi.fn(),
}));
vi.mock("../api/dragDrop", () => ({ onFilesDropped: vi.fn() }));
vi.mock("../api/events", () => ({ onAccountsChanged: vi.fn(), onLoginProgress: vi.fn() }));
// 3D の描画は jsdom では動かないので、渡された props だけを見えるようにした部品に差し替える
vi.mock("../components/skins/SkinPreview", () => ({
  SkinPreview: (p: { image: string | null; variant: string; capeImage: string | null }) => (
    <div data-testid="preview" data-image={p.image ?? ""} data-variant={p.variant} data-cape={p.capeImage ?? ""} />
  ),
}));
vi.mock("../api/opener", () => ({ openExternal: vi.fn() }));

const account = (patch: Partial<Account> = {}): Account => ({
  id: "a1", username: "Alex", isActive: true, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false, ...patch,
});

function mount(list: AccountList) {
  vi.mocked(listAccounts).mockResolvedValue(list);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Skins />
    </QueryClientProvider>,
  );
  return client;
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onFilesDropped).mockResolvedValue(() => {});
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(onLoginProgress).mockResolvedValue(() => {});
  vi.mocked(listSavedSkins).mockResolvedValue([]);
  vi.mocked(listDefaultSkins).mockResolvedValue([]);
  vi.mocked(getSavedSkinImage).mockResolvedValue("data:image/png;base64,AAAA");
  vi.mocked(importCurrentSkin).mockResolvedValue(null);
  vi.mocked(getCurrentSkin).mockResolvedValue({ image: null, sha256: null, variant: "classic", capes: [] });
  vi.mocked(getSkin).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  resetCooldown();
  vi.resetAllMocks();
});

describe("Skins", () => {
  it("shows only a login button when there are no accounts", async () => {
    mount({ accounts: [], devOffline: null });
    expect(await screen.findByRole("button", { name: "Add a Microsoft account" })).toBeTruthy();
    expect(screen.queryByText("Saved skins")).toBeNull();
    expect(document.querySelector("p")).toBeNull();
  });

  it("shows both section headings with an msa account and no login button", async () => {
    mount({ accounts: [account()], devOffline: null });
    expect(await screen.findByRole("heading", { name: "Saved skins" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Default skins" })).toBeTruthy();
    expect(screen.getByTestId("skin-preview-slot")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Sign in again" })).toBeNull();
  });

  it("shows the frame without a login button for the dev offline account only", async () => {
    mount({ accounts: [], devOffline: { username: "Player", active: true } });
    expect(await screen.findByRole("heading", { name: "Saved skins" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Sign in again" })).toBeNull();
  });

  it("offers the re-login at the top when the account needs login, and never the add-account flow", async () => {
    mount({ accounts: [account({ needsLogin: true })], devOffline: null });
    expect(await screen.findByRole("heading", { name: "Saved skins" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add a Microsoft account" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Sign in again" }));
    const dialog = await screen.findByRole("dialog");
    // 再ログインの中身（追加ではない）。対象のアカウントの名前も出る
    expect(within(dialog).getByRole("heading", { name: "Sign in to your Microsoft account again" })).toBeTruthy();
    expect(within(dialog).getByText("Alex")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Sign in with Microsoft" })).toBeTruthy();
  });

  it("keeps the dialog content while it is open even after the account stops needing login", async () => {
    const client = mount({ accounts: [account({ needsLogin: true })], devOffline: null });
    await screen.findByRole("heading", { name: "Saved skins" });
    fireEvent.click(screen.getByRole("button", { name: "Sign in again" }));
    const dialog = await screen.findByRole("dialog");
    act(() => client.setQueryData(ACCOUNTS_KEY, { accounts: [account()], devOffline: null }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Sign in again" })).toBeNull());
    expect(within(dialog).getByRole("heading", { name: "Sign in to your Microsoft account again" })).toBeTruthy();
    expect(within(dialog).queryByRole("heading", { name: "Add a Microsoft account" })).toBeNull();
  });

  it("shows the saved skins and the current skin with its active cape in the preview", async () => {
    vi.mocked(listSavedSkins).mockResolvedValue([
      { id: "s1", name: "First", variant: "classic", sha256: "h1", addedAtMs: 1 },
      { id: "s2", name: "Second", variant: "slim", sha256: "h2", addedAtMs: 2 },
    ]);
    vi.mocked(getCurrentSkin).mockResolvedValue({
      image: "data:image/png;base64,CUR",
      sha256: "h2",
      variant: "slim",
      capes: [
        { id: "c1", alias: "A", active: false, image: "data:image/png;base64,OFF" },
        { id: "c2", alias: "B", active: true, image: "data:image/png;base64,ON" },
      ],
    });
    mount({ accounts: [account()], devOffline: null });
    const second = await screen.findByRole("button", { name: "Second" });
    await waitFor(() => expect(second.getAttribute("aria-pressed")).toBe("true"));
    expect(screen.getByRole("button", { name: "First" }).getAttribute("aria-pressed")).toBe("false");
    const preview = await screen.findByTestId("preview");
    await waitFor(() => expect(preview.dataset.image).toBe("data:image/png;base64,CUR"));
    expect(preview.dataset.variant).toBe("slim");
    expect(preview.dataset.cape).toBe("data:image/png;base64,ON");
  });

  it("renders the cards but cannot press them when the account needs login", async () => {
    vi.mocked(listSavedSkins).mockResolvedValue([{ id: "s1", name: "First", variant: "classic", sha256: "h1", addedAtMs: 1 }]);
    mount({ accounts: [account({ needsLogin: true })], devOffline: null });
    const first = (await screen.findByRole("button", { name: "First" })) as HTMLButtonElement;
    expect(first.disabled).toBe(true);
  });

  it("shows the skeleton before the data arrives and never again after a later refetch", async () => {
    let release!: (l: AccountList) => void;
    vi.mocked(listAccounts).mockImplementation(() => new Promise((r) => (release = r)));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <Skins />
      </QueryClientProvider>,
    );
    await screen.findByTestId("skins-skeleton");
    expect(screen.queryByRole("heading", { name: "Saved skins" })).toBeNull();
    await act(async () => release({ accounts: [account()], devOffline: null }));
    expect(await screen.findByRole("heading", { name: "Saved skins" })).toBeTruthy();
    expect(screen.queryByTestId("skins-skeleton")).toBeNull();
    // 取り直しが長引いても（猶予より長く待っても）枠に戻らない
    vi.mocked(listAccounts).mockImplementation(() => new Promise(() => {}));
    await act(async () => {
      void client.invalidateQueries({ queryKey: ACCOUNTS_KEY });
      await new Promise((r) => setTimeout(r, 400));
    });
    expect(screen.queryByTestId("skins-skeleton")).toBeNull();
    expect(screen.getByRole("heading", { name: "Saved skins" })).toBeTruthy();
  });
});

const SKINS = [
  { id: "s1", name: "First", variant: "classic" as const, sha256: "h1", addedAtMs: 1 },
  { id: "s2", name: "Second", variant: "classic" as const, sha256: "h2", addedAtMs: 2 },
];
const switcher = () => screen.findByRole("button", { name: /Accounts/ }) as Promise<HTMLButtonElement>;
const checked = () =>
  screen
    .queryAllByRole("button")
    .filter((b) => b.getAttribute("aria-pressed") === "true")
    .map((b) => b.getAttribute("aria-label"));
const twoAccounts = (patch: Partial<Account> = {}): AccountList => ({
  accounts: [account(), account({ id: "a2", username: "Bob", isActive: false, ...patch })],
  devOffline: null,
});

describe("Skins account switching", () => {
  beforeEach(() => {
    vi.mocked(listSavedSkins).mockResolvedValue(SKINS);
    vi.mocked(getCurrentSkin).mockImplementation(async (id) => ({
      image: "data:image/png;base64,CUR",
      sha256: id === "a1" ? "h1" : "h2",
      variant: "classic",
      capes: [],
    }));
  });

  it("moves the check mark to the new account's skin and asks for that account's current skin", async () => {
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    expect(getCurrentSkin).toHaveBeenCalledWith("a1");
    fireEvent.click(await switcher());
    fireEvent.click(await screen.findByRole("option", { name: "Bob" }));
    await waitFor(() => expect(checked()).toEqual(["Second"]));
    expect(getCurrentSkin).toHaveBeenCalledWith("a2");
  });

  it("keeps the previous marks until the new account's skin arrives, then moves once (no empty step)", async () => {
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    let release!: () => void;
    vi.mocked(getCurrentSkin).mockImplementation(
      (id) =>
        new Promise((r) => {
          release = () => r({ image: "data:image/png;base64,B", sha256: id === "a1" ? "h1" : "h2", variant: "classic", capes: [] });
        }),
    );
    const seen: (string | null)[][] = [];
    const watch = new MutationObserver(() => seen.push(checked()));
    watch.observe(document.body, { subtree: true, attributes: true, attributeFilter: ["aria-pressed"] });
    fireEvent.click(await switcher());
    fireEvent.click(await screen.findByRole("option", { name: "Bob" }));
    await waitFor(() => expect(getCurrentSkin).toHaveBeenCalledWith("a2"));
    expect(checked()).toEqual(["First"]);
    // 借りている間は、このアカウントの現在のスキンとして取り込みを始めない
    expect(importCurrentSkin).not.toHaveBeenCalledWith("a2");
    await act(async () => release());
    await waitFor(() => expect(checked()).toEqual(["Second"]));
    watch.disconnect();
    expect(seen.every((names) => names.length === 1)).toBe(true);
    await waitFor(() => expect(importCurrentSkin).toHaveBeenCalledWith("a2"));
  });

  it("shows nothing of the previous account when switching to one that needs login", async () => {
    mount(twoAccounts({ needsLogin: true }));
    await waitFor(() => expect(checked()).toEqual(["First"]));
    fireEvent.click(await switcher());
    fireEvent.click(await screen.findByRole("option", { name: "Bob" }));
    await screen.findByRole("button", { name: "Sign in again" });
    expect(checked()).toEqual([]);
    expect((await screen.findByTestId("preview")).dataset.image).toBe("");
    expect(getCurrentSkin).not.toHaveBeenCalledWith("a2");
  });

  it("applies to the newly chosen account", async () => {
    vi.mocked(applySavedSkin).mockResolvedValue(null);
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    fireEvent.click(await switcher());
    fireEvent.click(await screen.findByRole("option", { name: "Bob" }));
    await waitFor(() => expect(checked()).toEqual(["Second"]));
    fireEvent.click(screen.getByRole("button", { name: "First" }));
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalledWith("a2", "s1"));
  });

  it("falls back to the active account when the picked account disappears", async () => {
    const client = mount(twoAccounts({ needsLogin: true }));
    await screen.findByRole("heading", { name: "Saved skins" });
    expect(screen.queryByRole("button", { name: "Sign in again" })).toBeNull();
    fireEvent.click(await switcher());
    fireEvent.click(await screen.findByRole("option", { name: "Bob" }));
    // 選んだのは要ログインのアカウント: 導線が出て、カードは押せない
    expect(await screen.findByRole("button", { name: "Sign in again" })).toBeTruthy();
    expect((screen.getByRole("button", { name: "First" }) as HTMLButtonElement).disabled).toBe(true);
    // そのアカウントが一覧から消える: 使用中のアカウントに戻り、導線が消え、押せる
    act(() => client.setQueryData(ACCOUNTS_KEY, { accounts: [account()], devOffline: null }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Sign in again" })).toBeNull());
    await waitFor(() => expect((screen.getByRole("button", { name: "Second" }) as HTMLButtonElement).disabled).toBe(false));
    await waitFor(() => expect(checked()).toEqual(["First"]));
  });

  it("disables the switcher while an apply is in flight and enables it again afterwards", async () => {
    let release!: () => void;
    vi.mocked(applySavedSkin).mockImplementation(() => new Promise((r) => (release = () => r(null))));
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    expect((await switcher()).disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Second" }));
    await waitFor(async () => expect((await switcher()).disabled).toBe(true));
    await act(async () => release());
    // 適用が済んでも冷却の間は止まったまま、明けると戻る
    await waitFor(() => expect(applySavedSkin).toHaveBeenCalled());
    expect((await switcher()).disabled).toBe(true);
    await waitFor(async () => expect((await switcher()).disabled).toBe(false), { timeout: CHANGE_COOLDOWN_MS + 1500 });
  }, CHANGE_COOLDOWN_MS + 5000);

  it("disables the switcher for the whole add batch", async () => {
    let handlers!: DragDropHandlers;
    vi.mocked(onFilesDropped).mockImplementation(async (h) => {
      handlers = h;
      return () => {};
    });
    let release!: () => void;
    vi.mocked(addSavedSkinFromPath).mockImplementation(() => new Promise((r) => (release = () => r({} as never))));
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    await waitFor(() => expect(handlers).toBeTruthy());
    await act(async () => handlers.drop(["a.png"]));
    await waitFor(async () => expect((await switcher()).disabled).toBe(true));
    await act(async () => release());
    await waitFor(async () => expect((await switcher()).disabled).toBe(false));
  });
});

describe("Skins default skins and the shared busy state", () => {
  const defaults = [{ key: "classic-steve", name: "Steve", variant: "classic" as const, image: "data:image/png;base64,AAAA", sha256: "d1" }];
  beforeEach(() => {
    vi.mocked(listSavedSkins).mockResolvedValue(SKINS);
    vi.mocked(listDefaultSkins).mockResolvedValue(defaults);
    vi.mocked(getCurrentSkin).mockImplementation(async () => ({ image: "data:image/png;base64,CUR", sha256: "h1", variant: "classic", capes: [] }));
  });
  const btn = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

  it("blocks the saved grid, the add card and the switcher while a default skin is being applied", async () => {
    let release!: () => void;
    vi.mocked(applyDefaultSkin).mockImplementation(() => new Promise((r) => (release = () => r(undefined as never))));
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    await screen.findByRole("button", { name: "Steve" });
    expect(btn("Second").disabled).toBe(false);
    fireEvent.click(btn("Steve"));
    await waitFor(() => expect(btn("Second").disabled).toBe(true));
    expect(btn("First").disabled).toBe(true);
    expect(btn("Add skin").disabled).toBe(true);
    expect((await switcher()).disabled).toBe(true);
    await act(async () => release());
    // 適用が済んでも冷却の間は、Mojang へ送る操作と切り替えが止まったまま。ライブラリの追加は続けられる
    await waitFor(() => expect(applyDefaultSkin).toHaveBeenCalled());
    expect(btn("Second").disabled).toBe(true);
    expect(btn("Add skin").disabled).toBe(false);
    expect((await switcher()).disabled).toBe(true);
  });

  it("keeps library edits open during the cooldown, but not a save that would change the worn skin", async () => {
    vi.mocked(applyDefaultSkin).mockResolvedValue(undefined as never);
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    fireEvent.click(await screen.findByRole("button", { name: "Steve" }));
    await waitFor(() => expect(applyDefaultSkin).toHaveBeenCalled());
    await waitFor(() => expect(btn("Second").disabled).toBe(true));

    const menus = screen.getAllByRole("button", { name: "Skin menu" }) as HTMLButtonElement[];
    expect(menus.every((m) => !m.disabled)).toBe(true);
    // 着ている First: モデルの切り替えは Mojang へ送るので保存できず、名前だけなら保存できる
    fireEvent.click(menus[0] as HTMLElement);
    const dialog = await screen.findByRole("dialog");
    const save = within(dialog).getByRole("button", { name: "Save" }) as HTMLButtonElement;
    fireEvent.click(within(dialog).getByRole("radio", { name: "Slim" }));
    expect(save.disabled).toBe(true);
    fireEvent.click(within(dialog).getByRole("radio", { name: "Classic" }));
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "Renamed" } });
    expect(save.disabled).toBe(false);
  });

  it("blocks the default skins while a saved skin is being applied", async () => {
    let release!: () => void;
    vi.mocked(applySavedSkin).mockImplementation(() => new Promise((r) => (release = () => r(null))));
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    await screen.findByRole("button", { name: "Steve" });
    expect(btn("Steve").disabled).toBe(false);
    fireEvent.click(btn("Second"));
    await waitFor(() => expect(btn("Steve").disabled).toBe(true));
    await act(async () => release());
    // 適用が済んでも冷却の間は止まったまま
    expect(btn("Steve").disabled).toBe(true);
  });

  it("keeps the saved cards in place while the default skins retry", async () => {
    vi.mocked(listDefaultSkins).mockRejectedValueOnce({ kind: "network", message: "x" });
    mount(twoAccounts());
    await waitFor(() => expect(checked()).toEqual(["First"]));
    const retry = await screen.findByRole("button", { name: "Retry" });
    const first = btn("First");
    let release!: (v: typeof defaults) => void;
    vi.mocked(listDefaultSkins).mockReturnValueOnce(new Promise((r) => (release = r)));
    fireEvent.click(retry);
    await waitFor(() => expect(retry.parentElement?.getAttribute("aria-busy")).toBe("true"));
    expect(btn("First")).toBe(first);
    await act(async () => release(defaults));
    await screen.findByRole("button", { name: "Steve" });
    expect(btn("First")).toBe(first);
    expect(checked()).toEqual(["First"]);
  });
});

describe("Skins when applying is not possible", () => {
  it("fires no request for an account that needs login and shows the empty preview frame", async () => {
    vi.mocked(listSavedSkins).mockResolvedValue(SKINS);
    mount({ accounts: [account({ needsLogin: true })], devOffline: null });
    const first = (await screen.findByRole("button", { name: "First" })) as HTMLButtonElement;
    fireEvent.click(first);
    fireEvent.click(screen.getByRole("button", { name: "Second" }));
    await new Promise((r) => setTimeout(r, 50));
    expect(applySavedSkin).not.toHaveBeenCalled();
    expect(importCurrentSkin).not.toHaveBeenCalled();
    expect(setCape).not.toHaveBeenCalled();
    expect(getCurrentSkin).not.toHaveBeenCalled();
    expect(checked()).toEqual([]);
    const preview = await screen.findByTestId("preview");
    expect(preview.dataset.image).toBe("");
    expect(preview.dataset.cape).toBe("");
  });

  it("lets a dev offline only state browse and add, but not apply", async () => {
    let handlers!: DragDropHandlers;
    vi.mocked(onFilesDropped).mockImplementation(async (h) => {
      handlers = h;
      return () => {};
    });
    vi.mocked(listSavedSkins).mockResolvedValue(SKINS);
    vi.mocked(addSavedSkinFromPath).mockResolvedValue({} as never);
    mount({ accounts: [], devOffline: { username: "Player", active: true } });
    const first = (await screen.findByRole("button", { name: "First" })) as HTMLButtonElement;
    expect(first.disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Add skin" }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByRole("button", { name: /Accounts/ })).toBeNull();
    fireEvent.click(first);
    await waitFor(() => expect(handlers).toBeTruthy());
    await act(async () => handlers.drop(["a.png"]));
    await waitFor(() => expect(addSavedSkinFromPath).toHaveBeenCalledWith("a.png"));
    expect(applySavedSkin).not.toHaveBeenCalled();
    expect(getCurrentSkin).not.toHaveBeenCalled();
    expect(importCurrentSkin).not.toHaveBeenCalled();
  });
});
