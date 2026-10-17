import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { beginLogin, cancelTask, getSkin, launchInstance, listAccounts } from "../../api/commands";
import { onAccountsChanged, onLoginProgress } from "../../api/events";
import { openExternal } from "../../api/opener";
import type { Account } from "../../bindings/Account";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { LoginProgress } from "../../bindings/LoginProgress";
import type { LoginProgressEvent } from "../../bindings/LoginProgressEvent";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import i18n from "../../i18n";
import { TASKS_KEY } from "../tasks/useTasks";
import { LoginDialog, type LoginDialogMode } from "./LoginDialog";
import { PROFILE_URL, PURCHASE_URL, TRIAL_URL, type LoginPrompt } from "./loginState";

vi.mock("../../api/commands", () => ({
  beginLogin: vi.fn(),
  cancelTask: vi.fn(),
  listAccounts: vi.fn(),
  getSkin: vi.fn(),
  launchInstance: vi.fn(),
}));
vi.mock("../../api/events", () => ({ onLoginProgress: vi.fn(), onAccountsChanged: vi.fn() }));
vi.mock("../../api/opener", () => ({ openExternal: vi.fn() }));

const account = (id: string, username: string, isActive: boolean): Account => ({
  id, username, isActive, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false,
});

let emit: (event: LoginProgressEvent) => void;
let client: QueryClient;
const onClose = vi.fn();

function mount(mode: LoginDialogMode, prompt?: LoginPrompt) {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, {});
  render(
    <QueryClientProvider client={client}>
      <LoginDialog mode={mode} prompt={prompt} instanceId="i1" onClose={onClose} />
    </QueryClientProvider>,
  );
}

function setTask(id: string, state: TaskSnapshot["state"], error: ErrorPayload | null = null) {
  act(() => {
    client.setQueryData(TASKS_KEY, (t: Record<string, TaskSnapshot> | undefined) => ({
      ...t,
      [id]: { id, kind: "login", instanceId: null, state, progress: { current: 0, total: 0 }, revision: 1, error, createdAt: 0, updatedAt: 0 },
    }));
  });
}

async function press(name: string | RegExp) {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name }));
    await Promise.resolve();
  });
}

/** 主のボタンを押して、ログインのタスク L1 を始める。 */
async function startLogin(name: string | RegExp = "Sign in with Microsoft and play") {
  await press(name);
  await waitFor(() => expect(beginLogin).toHaveBeenCalledWith("browser"));
  await act(async () => { await Promise.resolve(); });
}

function progress(p: LoginProgress, taskId = "L1") {
  act(() => emit({ taskId, progress: p }));
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onLoginProgress).mockImplementation(async (h) => {
    emit = h;
    return () => {};
  });
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(getSkin).mockResolvedValue(null);
  vi.mocked(openExternal).mockResolvedValue(undefined);
  vi.mocked(cancelTask).mockResolvedValue(null);
  vi.mocked(beginLogin).mockResolvedValue("L1");
  vi.mocked(launchInstance).mockResolvedValue("t2");
  vi.mocked(listAccounts).mockResolvedValue({ accounts: [account("acc1", "Steve", true)], devOffline: null });
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("LoginDialog", () => {
  it("login required dialog has no offline button", async () => {
    mount("play", { kind: "loginRequired", accountId: "acc1" });
    expect(screen.getByRole("heading", { name: "Sign in again to play" })).toBeTruthy();
    expect(await screen.findByText("Steve")).toBeTruthy();
    expect(screen.getByText("The game starts as soon as you've signed in")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Sign in with Microsoft and play" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Use a different account" }).getAttribute("href")).toBe("#/accounts");
    expect(screen.queryByRole("button", { name: "Play offline" })).toBeNull();
  });

  it("no account dialog asks to sign in", () => {
    mount("play", { kind: "noAccount" });
    expect(screen.getByRole("heading", { name: "Sign in with a Microsoft account" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Play offline" })).toBeNull();
  });

  it("offline dialog shows play offline only when available", async () => {
    mount("play", { kind: "offline", offlineAvailable: true });
    expect(screen.getByRole("heading", { name: "Can't connect to Microsoft" })).toBeTruthy();
    await press("Play offline");
    expect(launchInstance).toHaveBeenCalledWith("i1", true);
    expect(onClose).toHaveBeenCalled();
    cleanup();

    mount("play", { kind: "offline", offlineAvailable: false });
    expect(screen.queryByRole("button", { name: "Play offline" })).toBeNull();
    expect(screen.getByText("To play offline, ownership must be confirmed online once")).toBeTruthy();
  });

  it("offline dialog retry launches again", async () => {
    mount("play", { kind: "offline", offlineAvailable: false });
    await press("Retry");
    expect(launchInstance).toHaveBeenCalledWith("i1", false);
    expect(onClose).toHaveBeenCalled();
  });

  it("successful relogin of the same account launches the instance", async () => {
    mount("play", { kind: "loginRequired", accountId: "acc1" });
    await startLogin();
    setTask("L1", "succeeded");
    await waitFor(() => expect(launchInstance).toHaveBeenCalledWith("i1", false));
    expect(onClose).toHaveBeenCalled();
  });

  it("login with no account launches the new account", async () => {
    mount("play", { kind: "noAccount" });
    await startLogin();
    setTask("L1", "succeeded");
    await waitFor(() => expect(launchInstance).toHaveBeenCalledWith("i1", false));
  });

  it("login as a different account does not launch and says who logged in", async () => {
    vi.mocked(listAccounts).mockResolvedValue({
      accounts: [account("acc1", "Steve", false), account("acc2", "Alex", true)],
      devOffline: null,
    });
    mount("play", { kind: "loginRequired", accountId: "acc1" });
    await startLogin();
    setTask("L1", "succeeded");
    expect(await screen.findByText("Signed in as Alex")).toBeTruthy();
    expect(launchInstance).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    await press("Close");
    expect(onClose).toHaveBeenCalled();
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("waiting view offers reopen, device code and cancel", async () => {
    mount("play", { kind: "noAccount" });
    await startLogin();
    progress({ stage: "waitingForBrowser", url: "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?x" });
    expect(screen.getByText("Signing in with your browser…")).toBeTruthy();
    await press("Open the browser again");
    expect(openExternal).toHaveBeenCalledWith("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?x");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();

    setTask("L1", "running");
    await press("Sign in with a code");
    expect(cancelTask).toHaveBeenCalledWith("L1");
    setTask("L1", "cancelled");
    await waitFor(() => expect(beginLogin).toHaveBeenLastCalledWith("deviceCode"));
  });

  it("device code view shows a copyable code and opens only allowlisted URLs", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    mount("play", { kind: "noAccount" });
    await startLogin();
    progress({ stage: "deviceCode", userCode: "ABCD-1234", verificationUri: "https://www.microsoft.com/link" });
    const code = screen.getByText("ABCD-1234");
    expect(code.className).toContain("select-text");
    await press("Copy code");
    expect(writeText).toHaveBeenCalledWith("ABCD-1234");
    await press("Open the page");
    expect(openExternal).toHaveBeenCalledWith("https://www.microsoft.com/link");

    progress({ stage: "deviceCode", userCode: "ABCD-1234", verificationUri: "https://evil.example/link" });
    expect(screen.getByText("https://evil.example/link")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Open the page" })).toBeNull();
  });

  it("checking stages say the account is being checked", async () => {
    mount("play", { kind: "noAccount" });
    await startLogin();
    progress({ stage: "checkingXbox" });
    expect(screen.getByText("Checking your account…")).toBeTruthy();
    progress({ stage: "checkingMinecraft" });
    expect(screen.getByText("Checking your account…")).toBeTruthy();
  });

  it("cancel closes the dialog without an error", async () => {
    mount("play", { kind: "noAccount" });
    await startLogin();
    progress({ stage: "waitingForBrowser", url: "https://login.microsoftonline.com/x" });
    await press("Cancel");
    expect(cancelTask).toHaveBeenCalledWith("L1");
    expect(onClose).toHaveBeenCalled();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("login errors are shown inside the dialog", async () => {
    mount("play", { kind: "noAccount" });
    await startLogin();
    setTask("L1", "failed", { code: "auth.app_not_approved", params: {}, detail: null });
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("This app has not yet been approved to use the Minecraft API");
    expect(alert.textContent).toContain("This is a problem on the launcher's side, not with your account.");
    expect(screen.getByRole("dialog")).toBeTruthy();

    vi.mocked(beginLogin).mockResolvedValue("L2");
    await startLogin();
    setTask("L2", "failed", { code: "auth.not_owned", params: {}, detail: null });
    await screen.findByText("This account does not own Minecraft: Java Edition");
    await press("Buy Minecraft");
    expect(openExternal).toHaveBeenCalledWith(PURCHASE_URL);
    await press("Try the free trial");
    expect(openExternal).toHaveBeenCalledWith(TRIAL_URL);

    vi.mocked(beginLogin).mockResolvedValue("L3");
    await startLogin();
    setTask("L3", "failed", { code: "auth.no_profile", params: {}, detail: null });
    await screen.findByText("This account has no Minecraft profile");
    await press("Create a profile");
    expect(openExternal).toHaveBeenCalledWith(PROFILE_URL);
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("add mode closes on success without launching", async () => {
    mount("add");
    expect(screen.getByRole("heading", { name: "Add a Microsoft account" })).toBeTruthy();
    await startLogin("Sign in with Microsoft");
    setTask("L1", "succeeded");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("Esc during a running login cancels it and closes", async () => {
    mount("play", { kind: "noAccount" });
    await startLogin();
    progress({ stage: "waitingForBrowser", url: "https://login.microsoftonline.com/x" });
    await act(async () => {
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      await Promise.resolve();
    });
    expect(cancelTask).toHaveBeenCalledWith("L1");
    expect(onClose).toHaveBeenCalled();
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("is centred vertically", () => {
    mount("play", { kind: "noAccount" });
    const overlay = screen.getByRole("dialog").closest(".fixed");
    expect(overlay?.className).toContain("items-center");
    expect(overlay?.className).not.toContain("items-start");
  });
});
