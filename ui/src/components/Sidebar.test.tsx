import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getSkin, listAccounts } from "../api/commands";
import { onAccountsChanged } from "../api/events";
import type { Account } from "../bindings/Account";
import type { AccountList } from "../bindings/AccountList";
import i18n from "../i18n";
import { Sidebar } from "./Sidebar";

vi.mock("../api/commands", () => ({ listAccounts: vi.fn(), getSkin: vi.fn() }));
vi.mock("../api/events", () => ({ onAccountsChanged: vi.fn() }));

const account = (patch: Partial<Account> = {}): Account => ({
  id: "a1", username: "Alex", isActive: true, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false, ...patch,
});

function mount(list: AccountList) {
  vi.mocked(listAccounts).mockResolvedValue(list);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <Sidebar materialOn={false} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(getSkin).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("Sidebar", () => {
  it("sidebar shows the active account and a dot when login is needed", async () => {
    mount({ accounts: [account({ needsLogin: true })], devOffline: null });
    const link = await screen.findByRole("link", { name: "Alex (sign-in needed)" });
    expect(link.getAttribute("href")).toBe("/accounts");
    expect(link.querySelector(".bg-warning")).toBeTruthy();
    expect(link.querySelector("[data-testid='skin-face-default']")).toBeTruthy();
  });

  it("has no dot when the active account is fine", async () => {
    mount({ accounts: [account()], devOffline: null });
    const link = await screen.findByRole("link", { name: "Alex" });
    expect(link.querySelector(".bg-warning")).toBeNull();
  });

  it("shows the dev offline account when it is active", async () => {
    mount({ accounts: [account()], devOffline: { username: "Player", active: true } });
    expect(await screen.findByRole("link", { name: "Player" })).toBeTruthy();
  });

  it("sidebar shows a login link without an active account", async () => {
    mount({ accounts: [account({ isActive: false })], devOffline: null });
    const link = await screen.findByRole("link", { name: "Sign in" });
    expect(link.getAttribute("href")).toBe("/accounts");
  });

  it("has an item for the shared settings after the instances", async () => {
    mount({ accounts: [account()], devOffline: null });
    const nav = screen.getByRole("navigation");
    const link = within(nav).getByRole("link", { name: "Shared settings" });
    expect(link.getAttribute("href")).toBe("/sync");
    const names = within(nav).getAllByRole("link").map((l) => l.textContent);
    expect(names.indexOf("Shared settings")).toBe(names.indexOf("Instances") + 1);
    await act(async () => {
      await i18n.changeLanguage("ja");
    });
    expect(within(nav).getByRole("link", { name: "共通設定" })).toBeTruthy();
  });
});
