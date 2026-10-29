import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentSkin, getSavedSkinImage, importCurrentSkin, listAccounts, listSavedSkins } from "../api/commands";
import { onAccountsChanged, onLoginProgress } from "../api/events";
import type { Account } from "../bindings/Account";
import type { AccountList } from "../bindings/AccountList";
import i18n from "../i18n";
import Skins from "./Skins";

vi.mock("../api/commands", () => ({
  listAccounts: vi.fn(),
  listSavedSkins: vi.fn(),
  getCurrentSkin: vi.fn(),
  getSavedSkinImage: vi.fn(),
  importCurrentSkin: vi.fn(),
  applySavedSkin: vi.fn(),
  beginLogin: vi.fn(),
  cancelTask: vi.fn(),
  getSkin: vi.fn(),
}));
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
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(onLoginProgress).mockResolvedValue(() => {});
  vi.mocked(listSavedSkins).mockResolvedValue([]);
  vi.mocked(getSavedSkinImage).mockResolvedValue("data:image/png;base64,AAAA");
  vi.mocked(importCurrentSkin).mockResolvedValue(null);
  vi.mocked(getCurrentSkin).mockResolvedValue({ image: null, sha256: null, variant: "classic", capes: [] });
});

afterEach(() => {
  cleanup();
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
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("offers a login button at the top when the account needs login", async () => {
    mount({ accounts: [account({ needsLogin: true })], devOffline: null });
    expect(await screen.findByRole("heading", { name: "Saved skins" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Sign in again" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeTruthy());
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
});
