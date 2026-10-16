import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cancelTask, launchInstance, listRunning } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { LAST_EXIT_KEY, RUNNING_KEY, type LastExit } from "../features/play/useGameSync";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { PlayButton } from "./PlayButton";

vi.mock("../api/commands", () => ({
  launchInstance: vi.fn(),
  cancelTask: vi.fn(),
  listRunning: vi.fn().mockResolvedValue([]),
}));

const instance: Instance = {
  id: "i1", name: "World", dirName: "World", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
};

let client: QueryClient;

type MountOptions = {
  tasks?: TaskSnapshot[];
  sessions?: RunningSession[];
  lastExit?: LastExit;
  instance?: Instance;
};

function mount({ tasks = [], sessions = [], lastExit, instance: inst = instance }: MountOptions = {}) {
  client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  client.setQueryData(RUNNING_KEY, sessions);
  client.setQueryData(LAST_EXIT_KEY, lastExit ? { [lastExit.instanceId]: lastExit } : {});
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
    id: "t1", kind, instanceId: "i1", state: "running", progress: { current: 40, total: 100 }, revision: 1,
    error: null, createdAt: 0, updatedAt: Date.now(),
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
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

  it("shows playing with elapsed time and ignores presses", async () => {
    mount({ sessions: [{ instanceId: "i1", pid: 1, launchedAt: Date.now() - 65_000 }] });
    expect(await screen.findByRole("button", { name: "Playing" })).toBeTruthy();
    expect(screen.getByText("1:05")).toBeTruthy();
    await press("Playing");
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("shows a crash notice until Play is pressed again", async () => {
    vi.mocked(launchInstance).mockResolvedValue("t2");
    mount({ lastExit: { instanceId: "i1", exitCode: 1, at: Date.now() } });
    expect(await screen.findByText("The game crashed (exit code 1)")).toBeTruthy();
    await press("Play");
    await waitFor(() => expect(screen.queryByText("The game crashed (exit code 1)")).toBeNull());
    expect(launchInstance).toHaveBeenCalledWith("i1");
  });
});
