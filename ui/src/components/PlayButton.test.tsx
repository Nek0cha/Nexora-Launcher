import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cancelTask, startDemoTask } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { PlayButton } from "./PlayButton";

vi.mock("../api/commands", () => ({ startDemoTask: vi.fn(), cancelTask: vi.fn() }));

let client: QueryClient;

function mount(tasks: TaskSnapshot[] = []) {
  client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  return render(
    <QueryClientProvider client={client}>
      <PlayButton />
    </QueryClientProvider>,
  );
}

function running(): TaskSnapshot {
  return {
    id: "t1", kind: "demo", state: "running", progress: { current: 40, total: 100 }, revision: 1,
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
  vi.mocked(startDemoTask).mockReset();
  vi.mocked(cancelTask).mockReset();
});

describe("PlayButton", () => {
  it("shows the error when starting fails, and clears it on the next successful start", async () => {
    vi.mocked(startDemoTask).mockRejectedValueOnce(
      new IpcError({ code: "app.not_ready", params: {}, detail: "secret detail" }),
    );
    mount();
    await press("Play");
    expect(await screen.findByText("Startup has not finished yet")).toBeTruthy();
    expect(screen.queryByText("secret detail")).toBeNull();
    await press("Details");
    expect(screen.getByText("secret detail")).toBeTruthy();

    vi.mocked(startDemoTask).mockResolvedValueOnce("t2");
    await press("Play");
    await act(async () => { await Promise.resolve(); });
    expect(screen.queryByText("Startup has not finished yet")).toBeNull();
  });

  it("shows the error when cancelling fails", async () => {
    vi.mocked(cancelTask).mockRejectedValueOnce(new IpcError({ code: "task.not_found", params: {}, detail: null }));
    mount([running()]);
    await press("Cancel");
    expect(await screen.findByText("The task was not found")).toBeTruthy();
  });

  it("sends one cancel while a cancel request is in flight", async () => {
    const pending = deferred<null>();
    vi.mocked(cancelTask).mockReturnValue(pending.promise);
    mount([running()]);
    await press("Cancel");
    // 応答前の 2 回目の押下（表示が切り替わる前でも後でも）は送らない
    await press(/^Cancel/);
    expect(await screen.findByRole("button", { name: "Cancelling" })).toBeTruthy();
    await press(/^Cancel/);
    expect(cancelTask).toHaveBeenCalledTimes(1);
    await act(async () => { pending.resolve(null); await pending.promise; });
  });
});
