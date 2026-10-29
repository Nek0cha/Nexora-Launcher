import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  beginLogin,
  cancelTask,
  getSkin,
  listAccounts,
  removeAccount,
  setActiveAccount,
  setDevOfflineActive,
  setDevOfflineUsername,
} from "../api/commands";
import { onAccountsChanged, onLoginProgress } from "../api/events";
import { IpcError } from "../api/ipc";
import type { Account } from "../bindings/Account";
import type { AccountList } from "../bindings/AccountList";
import { ToastRegion } from "../components/toast/ToastRegion";
import { clearToasts } from "../components/toast/useToast";
import { ACCOUNTS_KEY } from "../features/accounts/useAccounts";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import Accounts from "./Accounts";

vi.mock("../api/commands", () => ({
  listAccounts: vi.fn(),
  setActiveAccount: vi.fn(),
  setDevOfflineActive: vi.fn(),
  setDevOfflineUsername: vi.fn(),
  removeAccount: vi.fn(),
  beginLogin: vi.fn(),
  cancelTask: vi.fn(),
  getSkin: vi.fn(),
  launchInstance: vi.fn(),
}));
vi.mock("../api/events", () => ({ onAccountsChanged: vi.fn(), onLoginProgress: vi.fn() }));
vi.mock("../api/opener", () => ({ openExternal: vi.fn() }));

const account = (id: string, username: string, patch: Partial<Account> = {}): Account => ({
  id, username, isActive: false, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false, ...patch,
});

let client: QueryClient;

function mount(list: AccountList) {
  vi.mocked(listAccounts).mockResolvedValue(list);
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, {});
  render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <Accounts />
        <ToastRegion />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

const row = (name: string) => screen.findByRole("listitem", { name });

async function press(el: HTMLElement) {
  await act(async () => {
    fireEvent.click(el);
    await Promise.resolve();
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(onLoginProgress).mockResolvedValue(() => {});
  vi.mocked(getSkin).mockResolvedValue(null);
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

describe("Accounts", () => {
  it("lists accounts with face, name and status", async () => {
    mount({
      accounts: [
        account("a1", "Alex", { isActive: true }),
        account("a2", "Steve", { needsLogin: true }),
        account("a3", "Notch", { loginSaved: false }),
        account("a4", "Jeb", { entitlement: "unknown" }),
      ],
      devOffline: null,
    });
    const alex = await row("Alex");
    expect(within(alex).getByText("In use")).toBeTruthy();
    expect(within(alex).getByTestId("skin-face-default")).toBeTruthy();
    expect(within(await row("Steve")).getByText("Sign-in needed")).toBeTruthy();
    expect(within(await row("Notch")).getByText("Sign-in not saved")).toBeTruthy();
    expect(within(await row("Jeb")).getByText("Ownership not confirmed")).toBeTruthy();
  });

  it("keeps a same-size slot where Use is, so Change skin and Remove do not move when the row becomes active", async () => {
    mount({
      accounts: [account("a1", "Alex", { isActive: true }), account("a2", "Steve", { isActive: false })],
      devOffline: null,
    });
    const active = await row("Alex");
    const idle = await row("Steve");
    const slot = active.querySelector("[data-action-slot]") as HTMLElement;
    expect(slot).not.toBeNull();
    expect(slot.getAttribute("aria-hidden")).toBe("true");
    expect(idle.querySelector("[data-action-slot]")).toBeNull();
    const use = within(idle).getByRole("button", { name: "Use" });
    // 同じ寸法のクラス（高さ、余白、文字）で、同じ文字を持つ
    const size = (el: HTMLElement) => el.className.split(/\s+/).filter((c) => /^(h-|px-|text-(xs|sm))/.test(c));
    expect(size(slot)).toEqual(size(use));
    expect(slot.textContent).toBe(use.textContent);
    // ボタンの数ではなく、並びの数が同じ
    const actions = (r: HTMLElement) => (r.querySelector("div.shrink-0") as HTMLElement).children.length;
    expect(actions(active)).toBe(actions(idle));
  });

  it("use, relogin and remove actions per row", async () => {
    // 「使う」は使用中でない行だけ、「ログインし直す」は needsLogin の行だけ
    vi.mocked(setActiveAccount).mockResolvedValue(null);
    mount({
      accounts: [account("a1", "Alex", { isActive: true }), account("a2", "Steve", { needsLogin: true })],
      devOffline: null,
    });
    const alex = await row("Alex");
    expect(within(alex).queryByRole("button", { name: "Use" })).toBeNull();
    expect(within(alex).queryByRole("button", { name: "Sign in again" })).toBeNull();
    expect(within(alex).getByRole("button", { name: "Remove" })).toBeTruthy();

    const steve = await row("Steve");
    expect(within(steve).getByRole("button", { name: "Remove" })).toBeTruthy();
    await press(within(steve).getByRole("button", { name: "Use" }));
    expect(setActiveAccount).toHaveBeenCalledWith("a2");

    await press(within(steve).getByRole("button", { name: "Sign in again" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Sign in to your Microsoft account again")).toBeTruthy();
    expect(within(dialog).getByText("Steve")).toBeTruthy();
  });

  it("remove asks for confirmation and explains that tokens are not revoked", async () => {
    vi.mocked(removeAccount).mockResolvedValue(null);
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null });
    await press(within(await row("Alex")).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("Remove Alex?")).toBeTruthy();
    expect(
      within(dialog).getByText(
        "This removes the sign-in information from this computer. Tokens that Microsoft has already issued are not revoked.",
      ),
    ).toBeTruthy();
    expect(removeAccount).not.toHaveBeenCalled();
    await press(within(dialog).getByRole("button", { name: "Remove" }));
    expect(removeAccount).toHaveBeenCalledWith("a1");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("the remove confirmation is described by its body text", async () => {
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null });
    await press(within(await row("Alex")).getByRole("button", { name: "Remove" }));
    expect(
      await screen.findByRole("alertdialog", {
        name: "Remove Alex?",
        description:
          "This removes the sign-in information from this computer. Tokens that Microsoft has already issued are not revoked.",
      }),
    ).toBeTruthy();
  });

  it("cancel in the remove confirmation keeps the account", async () => {
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null });
    await press(within(await row("Alex")).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("alertdialog");
    await press(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(removeAccount).not.toHaveBeenCalled();
  });

  it("shows account_in_use when the account is used by a running game", async () => {
    vi.mocked(removeAccount).mockRejectedValue(
      new IpcError({ code: "auth.account_in_use", params: {}, detail: null }),
    );
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null });
    await press(within(await row("Alex")).getByRole("button", { name: "Remove" }));
    await press(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    const alert = await within(screen.getByRole("region", { name: "Notifications" })).findByRole("alert");
    expect(within(alert).getByText("This account can't be removed while a game is running")).toBeTruthy();
  });

  it("empty state has an add button", async () => {
    mount({ accounts: [], devOffline: null });
    expect(await screen.findByText("Add a Microsoft account to play Minecraft.")).toBeTruthy();
    await press(screen.getByRole("button", { name: "Add a Microsoft account" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Add a Microsoft account" })).toBeTruthy();
  });

  it("the add dialog survives the switch from the empty state to the list", async () => {
    // accounts://changed がタスクの成功より先に届いて一覧に切り替わっても、ダイアログを作り直さない（作り直すとログインがキャンセルされる）
    vi.mocked(beginLogin).mockResolvedValue("L1");
    vi.mocked(cancelTask).mockResolvedValue(null);
    mount({ accounts: [], devOffline: null });
    await press(await screen.findByRole("button", { name: "Add a Microsoft account" }));
    await press(within(await screen.findByRole("dialog")).getByRole("button", { name: "Sign in with Microsoft" }));
    await act(async () => { await Promise.resolve(); });
    expect(await screen.findByText("Getting ready…")).toBeTruthy();

    const added = { accounts: [account("a1", "Alex", { isActive: true })], devOffline: null };
    vi.mocked(listAccounts).mockResolvedValue(added);
    act(() => client.setQueryData(ACCOUNTS_KEY, added));
    expect(await screen.findByRole("listitem", { name: "Alex", hidden: true })).toBeTruthy();
    expect(within(screen.getByRole("dialog")).getByText("Getting ready…")).toBeTruthy();
    expect(cancelTask).not.toHaveBeenCalled();

    act(() => {
      client.setQueryData(TASKS_KEY, {
        L1: { id: "L1", kind: "login", instanceId: null, state: "succeeded", phase: null, progress: { current: 0, total: 0 }, revision: 1, error: null, createdAt: 0, updatedAt: 0 },
      });
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("add without a client ID shows auth.no_client_id", async () => {
    vi.mocked(beginLogin).mockRejectedValue(new IpcError({ code: "auth.no_client_id", params: {}, detail: null }));
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null });
    await row("Alex");
    await press(screen.getByRole("button", { name: "Add a Microsoft account" }));
    const dialog = await screen.findByRole("dialog");
    await press(within(dialog).getByRole("button", { name: "Sign in with Microsoft" }));
    expect(beginLogin).toHaveBeenCalledWith("browser");
    expect(
      await within(dialog).findByText("This build has no Microsoft client ID, so sign-in is unavailable"),
    ).toBeTruthy();
  });

  it("dev build shows the dev offline row with the name field", async () => {
    vi.mocked(setDevOfflineActive).mockResolvedValue(null);
    vi.mocked(setDevOfflineUsername).mockResolvedValue("Steve_1");
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: { username: "Player", active: false } });
    const dev = await row("Development offline account");
    const input = within(dev).getByRole("textbox", { name: "Development offline account" }) as HTMLInputElement;
    expect(input.value).toBe("Player");
    expect(input.maxLength).toBe(16);
    expect(within(dev).queryByRole("button", { name: "Remove" })).toBeNull();

    await press(within(dev).getByRole("button", { name: "Use" }));
    expect(setDevOfflineActive).toHaveBeenCalledWith(true);

    fireEvent.change(input, { target: { value: "Steve_1" } });
    await press(within(dev).getByRole("button", { name: "Save" }));
    expect(setDevOfflineUsername).toHaveBeenCalledWith("Steve_1");
    expect(await within(dev).findByText("Saved")).toBeTruthy();
  });

  it("dev offline row shows the invalid name error from the backend", async () => {
    vi.mocked(setDevOfflineUsername).mockRejectedValue(
      new IpcError({ code: "settings.invalid_value", params: {}, detail: null }),
    );
    mount({ accounts: [], devOffline: { username: "Player", active: true } });
    const dev = await row("Development offline account");
    expect(within(dev).getByText("In use")).toBeTruthy();
    expect(within(dev).queryByRole("button", { name: "Use" })).toBeNull();
    const input = within(dev).getByRole("textbox", { name: "Development offline account" }) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "bad name!" } });
    await press(within(dev).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("The setting value is invalid")).toBeTruthy();
    expect(input.value).toBe("bad name!");
    // 入力の誤りはその場に出し、トーストにはしない
    expect(within(dev).getByText("The setting value is invalid")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Notifications" })).queryByRole("alert")).toBeNull();
  });

  it("release build has no dev offline row", async () => {
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null });
    await row("Alex");
    expect(screen.queryByText("Development offline account")).toBeNull();
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("links_to_the_skins_screen_only_on_usable_microsoft_rows", async () => {
    mount({
      accounts: [account("a1", "Alex", { isActive: true }), account("a2", "Sam", { needsLogin: true })],
      devOffline: { username: "Player", active: false },
    });
    const alex = within(await row("Alex"));
    expect(alex.getByRole("link", { name: "Change skin" }).getAttribute("href")).toBe("/skins");
    expect(alex.queryByRole("button", { name: "Change skin" })).toBeNull();
    // ログインし直しが必要な行には出さない
    const sam = within(await row("Sam"));
    expect(sam.queryByRole("link", { name: "Change skin" })).toBeNull();
    // 開発用のオフラインアカウントの行には出さない
    const dev = within(await row("Development offline account"));
    expect(dev.queryByRole("link", { name: "Change skin" })).toBeNull();
  });
});
