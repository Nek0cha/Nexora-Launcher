import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cancelTask, launchInstance, listAccounts, listRunning, stopInstance } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { LAST_EXIT_KEY, RUNNING_KEY, type LastExit } from "../features/play/useGameSync";
import { STOP_KEY } from "../features/play/useStopFlow";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { PlayButton } from "./PlayButton";

vi.mock("../api/commands", () => ({
  launchInstance: vi.fn(),
  cancelTask: vi.fn(),
  stopInstance: vi.fn(),
  listRunning: vi.fn().mockResolvedValue([]),
  listAccounts: vi.fn(),
  getSkin: vi.fn().mockResolvedValue(null),
  beginLogin: vi.fn(),
}));
vi.mock("../api/events", () => ({
  onLoginProgress: vi.fn().mockResolvedValue(() => {}),
  onAccountsChanged: vi.fn().mockResolvedValue(() => {}),
}));

const instance: Instance = {
  id: "i1", name: "World", dirName: "World", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

let client: QueryClient;

type MountOptions = {
  tasks?: TaskSnapshot[];
  sessions?: RunningSession[];
  lastExit?: LastExit;
  instance?: Instance;
  stop?: Record<string, "waiting" | "forceReady">;
};

function mount({ tasks = [], sessions = [], lastExit, instance: inst = instance, stop = {} }: MountOptions = {}) {
  client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  client.setQueryData(RUNNING_KEY, sessions);
  client.setQueryData(LAST_EXIT_KEY, lastExit ? { [lastExit.instanceId]: lastExit } : {});
  client.setQueryData(STOP_KEY, { states: stop, confirm: [] });
  // 取り直しでも同じ内容が返るようにする
  vi.mocked(listRunning).mockResolvedValue(sessions);
  return render(
    <QueryClientProvider client={client}>
      <PlayButton instance={inst} />
    </QueryClientProvider>,
  );
}

function running(kind = "launch"): TaskSnapshot {
  return {
    id: "t1", kind, instanceId: "i1", state: "running", phase: null, progress: { current: 40, total: 100 }, revision: 1,
    error: null, createdAt: 0, updatedAt: Date.now(),
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

/** ボタンの面に見えている進み具合の文言（数字は別の要素に分かれる）。 */
function shownLabel(text: string) {
  return screen.getByText((_, el) =>
    el?.parentElement?.matches("button > [aria-hidden]") === true && el.textContent === text);
}

async function press(name: string | RegExp) {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name }));
    await Promise.resolve();
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.mocked(launchInstance).mockReset();
  vi.mocked(cancelTask).mockReset();
  vi.mocked(stopInstance).mockReset();
});

describe("PlayButton", () => {
  it("shows the error when starting fails, and clears it on the next successful start", async () => {
    vi.mocked(launchInstance).mockRejectedValueOnce(
      new IpcError({ code: "app.not_ready", params: {}, detail: "secret detail" }),
    );
    mount();
    await press("Play");
    expect(await screen.findByText("Startup has not finished yet")).toBeTruthy();
    expect(screen.queryByText("secret detail")).toBeNull();
    await press("Details");
    expect(screen.getByText("secret detail")).toBeTruthy();

    vi.mocked(launchInstance).mockResolvedValueOnce("t2");
    await press("Play");
    await act(async () => { await Promise.resolve(); });
    expect(screen.queryByText("Startup has not finished yet")).toBeNull();
  });

  it("shows the error when cancelling fails", async () => {
    vi.mocked(cancelTask).mockRejectedValueOnce(new IpcError({ code: "task.not_found", params: {}, detail: null }));
    mount({ tasks: [running()] });
    await press("Cancel");
    expect(await screen.findByText("The task was not found")).toBeTruthy();
  });

  it("sends one cancel while a cancel request is in flight", async () => {
    const pending = deferred<null>();
    vi.mocked(cancelTask).mockReturnValue(pending.promise);
    mount({ tasks: [running()] });
    await press("Cancel");
    // 応答前の 2 回目の押下（表示が切り替わる前でも後でも）は送らない
    await press(/^Cancel/);
    expect(await screen.findByRole("button", { name: "Cancelling" })).toBeTruthy();
    await press(/^Cancel/);
    expect(cancelTask).toHaveBeenCalledTimes(1);
    await act(async () => { pending.resolve(null); await pending.promise; });
  });

  it("does not start a game until the running list has loaded", async () => {
    mount();
    // 一覧の応答を保留にして、受信前の状態を作る
    cleanup();
    client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    vi.mocked(listRunning).mockReturnValue(new Promise(() => {}));
    render(
      <QueryClientProvider client={client}>
        <PlayButton instance={instance} />
      </QueryClientProvider>,
    );
    const button = screen.getByRole("button", { name: "Play" });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    await press("Play");
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("offers installation for an instance that is not ready", () => {
    mount({ instance: { ...instance, state: "broken" } });
    expect(screen.getByRole("button", { name: "Install" })).toBeTruthy();
  });

  it("labels install and launch progress differently", () => {
    mount({ tasks: [running("install")] });
    expect(screen.getByText("Preparing 40%")).toBeTruthy();
    cleanup();
    mount({ tasks: [running("launch")] });
    expect(screen.getByText("Launching 40%")).toBeTruthy();
  });

  it("names the task phase with its number when there is one", async () => {
    const at = (phase: TaskSnapshot["phase"], current: number, total: number): TaskSnapshot =>
      ({ ...running("install"), phase, progress: { current, total } });
    const cases: [TaskSnapshot, string][] = [
      [at("resolving", 0, 0), "Fetching info"],
      [at("verifying", 42, 100), "Verifying files 42%"],
      [at("downloading", 63, 100), "Downloading 63%"],
      [at("downloading", 0, 0), "Downloading"],
      [at("building_loader", 2, 5), "Building loader 2/5"],
      [at("finishing", 0, 0), "Finishing"],
      [at("launching", 0, 0), "Launching"],
    ];
    for (const [task, label] of cases) {
      mount({ tasks: [task] });
      expect(shownLabel(label)).toBeTruthy();
      // 読み上げの説明も同じ文言
      const button = screen.getByRole("button", { name: "Cancel" });
      expect(document.getElementById(button.getAttribute("aria-describedby")!)?.textContent).toBe(label);
      cleanup();
    }
    await act(async () => {
      await i18n.changeLanguage("ja");
    });
    mount({ tasks: [at("building_loader", 2, 5)] });
    expect(shownLabel("ローダーを構築中 2/5")).toBeTruthy();
    cleanup();
    mount({ tasks: [at("verifying", 42, 100)] });
    expect(shownLabel("ファイルを確認中 42%")).toBeTruthy();
  });

  it("moves the bar without a known amount while the phase has no percent", () => {
    const at = (phase: TaskSnapshot["phase"], current: number, total: number): TaskSnapshot =>
      ({ ...running("install"), phase, progress: { current, total } });
    for (const task of [at("resolving", 0, 0), at("building_loader", 2, 5), at("finishing", 0, 0), at("downloading", 0, 0)]) {
      mount({ tasks: [task] });
      expect(screen.getByTestId("play-fill").dataset.indeterminate).toBe("true");
      cleanup();
    }
    mount({ tasks: [at("downloading", 63, 100)] });
    const fill = screen.getByTestId("play-fill");
    expect(fill.dataset.indeterminate).toBeUndefined();
    expect(fill.style.transform).toBe("scaleX(0.63)");
  });

  it("shows playing with elapsed time and does not launch again", async () => {
    vi.mocked(stopInstance).mockResolvedValue(null);
    mount({ sessions: [{ instanceId: "i1", pid: 1, launchedAt: Date.now() - 65_000 }] });
    const button = await screen.findByRole("button", { name: "Stop" });
    expect(button.getAttribute("aria-describedby")).toBeTruthy();
    expect(screen.getByText("Playing", { selector: "[aria-hidden] span" })).toBeTruthy();
    expect(screen.getByText("1:05")).toBeTruthy();
    await press("Stop");
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("playing_button_shows_stop_on_hover_and_calls_request_stop", async () => {
    vi.mocked(stopInstance).mockResolvedValue(null);
    mount({ sessions: [{ instanceId: "i1", pid: 1, launchedAt: Date.now() }] });
    const button = await screen.findByRole("button", { name: "Stop" });
    // 二つの層は常に同じ要素の中にあり、表示は不透明度で切り替わる
    const stopLayer = within(button).getByTestId("play-layer-stop");
    const playingLayer = within(button).getByTestId("play-layer-playing");
    expect(stopLayer.className).toContain("opacity-0");
    expect(playingLayer.className).not.toContain("opacity-0");
    expect(button.className).not.toContain("bg-danger");
    await act(async () => {
      fireEvent.pointerEnter(button, { pointerType: "mouse" });
    });
    expect(within(button).getByTestId("play-layer-stop").className).not.toContain("opacity-0");
    expect(within(button).getByTestId("play-layer-playing").className).toContain("opacity-0");
    expect(button.className).toContain("bg-danger");
    expect(button.className).toContain("transition-colors");
    await press("Stop");
    expect(stopInstance).toHaveBeenCalledWith("i1", false);
    expect(await screen.findByRole("button", { name: "Waiting for the game to close" })).toBeTruthy();
  });

  it("waiting_shows_waiting_label", async () => {
    mount({ sessions: [{ instanceId: "i1", pid: 1, launchedAt: Date.now() }], stop: { i1: "waiting" } });
    const button = await screen.findByRole("button", { name: "Waiting for the game to close" });
    expect(within(button).getByText("Waiting for the game to close")).toBeTruthy();
    expect(button.getAttribute("aria-disabled")).toBe("true");
    await act(async () => {
      fireEvent.pointerEnter(button, { pointerType: "mouse" });
    });
    expect(button.className).not.toContain("bg-danger");
    await press("Waiting for the game to close");
    expect(stopInstance).not.toHaveBeenCalled();
  });

  it("after the confirmation is dismissed, pressing force stops", async () => {
    vi.mocked(stopInstance).mockResolvedValue(null);
    mount({ sessions: [{ instanceId: "i1", pid: 1, launchedAt: Date.now() }], stop: { i1: "forceReady" } });
    await press("Force stop");
    expect(stopInstance).toHaveBeenCalledWith("i1", true);
  });

  it("shows a crash notice until Play is pressed again", async () => {
    vi.mocked(launchInstance).mockResolvedValue("t2");
    mount({ lastExit: { instanceId: "i1", exitCode: 1, at: Date.now() } });
    expect(await screen.findByText("The game crashed (exit code 1)")).toBeTruthy();
    await press("Play");
    await waitFor(() => expect(screen.queryByText("The game crashed (exit code 1)")).toBeNull());
    expect(launchInstance).toHaveBeenCalledWith("i1", false);
  });

  it("PlayButton opens the dialog instead of showing a failure", async () => {
    vi.mocked(listAccounts).mockResolvedValue({
      accounts: [{ id: "acc1", username: "Steve", isActive: true, entitlement: "verified", needsLogin: true, loginSaved: true, hasSkin: false }],
      devOffline: null,
    });
    vi.mocked(launchInstance).mockResolvedValue("t5");
    mount();
    await press("Play");
    act(() => {
      client.setQueryData(TASKS_KEY, {
        t5: {
          id: "t5", kind: "launch", instanceId: "i1", state: "failed", phase: null, progress: { current: 0, total: 0 }, revision: 2,
          error: { code: "auth.login_required", params: { accountId: "acc1" }, detail: null },
          createdAt: 0, updatedAt: Date.now(),
        },
      });
    });
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("Sign in again to play");
    // プレイボタンの下（上）に失敗の説明を出さない
    expect(screen.queryByText("Please sign in again")).toBeNull();

    await press("Cancel");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.queryByText("Please sign in again")).toBeNull();
    // フォーカスはプレイボタンに戻る
    await waitFor(() => expect(document.activeElement?.getAttribute("aria-label")).toBe("Try again"));
  });

  it("also opens the dialog when the start call itself fails with an auth error", async () => {
    vi.mocked(listAccounts).mockResolvedValue({ accounts: [], devOffline: null });
    vi.mocked(launchInstance).mockRejectedValueOnce(new IpcError({ code: "auth.no_account", params: {}, detail: null }));
    mount();
    await press("Play");
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("Sign in with a Microsoft account");
    expect(screen.queryByText("No account is signed in")).toBeNull();
  });
});
