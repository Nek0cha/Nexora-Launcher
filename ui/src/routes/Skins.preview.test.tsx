import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentSkin, getSavedSkinImage, importCurrentSkin, listAccounts, listDefaultSkins, listSavedSkins } from "../api/commands";
import { onAccountsChanged, onLoginProgress } from "../api/events";
import i18n from "../i18n";
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
  beginLogin: vi.fn(),
  cancelTask: vi.fn(),
  getSkin: vi.fn(),
}));
vi.mock("../api/events", () => ({ onAccountsChanged: vi.fn(), onLoginProgress: vi.fn() }));
vi.mock("../api/opener", () => ({ openExternal: vi.fn() }));
// プレビューの塊の読み込みが失敗する（404、オフライン、壊れたキャッシュ）。本物の lazy の経路を通す
vi.mock("../components/skins/SkinPreview", () => {
  throw new Error("chunk 404");
});

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(onLoginProgress).mockResolvedValue(() => {});
  vi.mocked(listDefaultSkins).mockResolvedValue([]);
  vi.mocked(listAccounts).mockResolvedValue({
    accounts: [{ id: "a1", username: "Alex", isActive: true, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false }],
    devOffline: null,
  });
  vi.mocked(listSavedSkins).mockResolvedValue([{ id: "s1", name: "First", variant: "classic", sha256: "h1", addedAtMs: 1 }]);
  vi.mocked(getSavedSkinImage).mockResolvedValue("data:image/png;base64,AAAA");
  vi.mocked(importCurrentSkin).mockResolvedValue(null);
  vi.mocked(getCurrentSkin).mockResolvedValue({ image: null, sha256: null, variant: "classic", capes: [] });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.resetAllMocks();
});

describe("Skins when the preview chunk fails to load", () => {
  it("still shows the saved skins, with an empty preview frame and one warning", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <Skins />
      </QueryClientProvider>,
    );
    expect(await screen.findByRole("button", { name: "First" })).toBeTruthy();
    await waitFor(() => expect(warn).toHaveBeenCalledWith("SkinPreview: chunk load failed", expect.anything()));
    expect(warn.mock.calls.filter((c) => c[0] === "SkinPreview: chunk load failed")).toHaveLength(1);
    expect(screen.getByTestId("skin-preview-slot").textContent).toBe("");
    expect(screen.getByRole("heading", { name: "Saved skins" })).toBeTruthy();
  });
});
