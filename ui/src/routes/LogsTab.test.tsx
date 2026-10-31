import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listGameLogs, listRunning, openInstanceFolder, readGameLog } from "../api/commands";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import { ToastRegion } from "../components/toast/ToastRegion";
import { clearToasts } from "../components/toast/useToast";
import i18n from "../i18n";
import { LogsTab, MAX_RENDERED_LINES } from "./LogsTab";

vi.mock("../api/commands", () => ({
  listGameLogs: vi.fn(),
  readGameLog: vi.fn(),
  listRunning: vi.fn(),
  openInstanceFolder: vi.fn(),
}));

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

const logs = [
  { name: "2000.log", launchedAt: 2000, size: 10 },
  { name: "1000.log", launchedAt: 1000, size: 10 },
];
const sample = [
  "[12:00:00 UTC] [main/INFO]: Loading",
  "[12:00:01 UTC] [Render thread/WARN]: Slow frame",
  "[12:00:02 UTC] [Render thread/ERROR]: Crash",
  "\tat net.minecraft.Foo.bar(Foo.java:1)",
  "",
].join("\n");

const session: RunningSession = { instanceId: "i1", pid: 1, launchedAt: 2000, accountId: "a" } as RunningSession;

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <LogsTab instance={instance} />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

const view = () => screen.findByRole("log", { name: "Game log" });
const lineTexts = (el: HTMLElement) => Array.from(el.children).map((c) => c.textContent);

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(listRunning).mockResolvedValue([]);
  vi.mocked(listGameLogs).mockResolvedValue(logs);
  vi.mocked(readGameLog).mockResolvedValue({ text: sample, truncated: false });
});

afterEach(() => {
  cleanup();
  clearToasts();
  vi.useRealTimers();
  vi.resetAllMocks();
});

describe("LogsTab", () => {
  it("shows the newest launch first, with each line colored by level", async () => {
    mount();
    const log = await view();
    await waitFor(() => expect(lineTexts(log)).toHaveLength(4));
    expect(readGameLog).toHaveBeenCalledWith("i1", "2000.log");
    expect(log.children[1]?.className).toContain("text-warning");
    expect(log.children[2]?.className).toContain("text-danger");
    // スタックトレースの続きは、直前のエラーの色になる
    expect(log.children[3]?.className).toContain("text-danger");
    expect(log.children[0]?.className).toContain("text-text");
  });

  it("says so when there are no logs", async () => {
    vi.mocked(listGameLogs).mockResolvedValue([]);
    mount();
    expect(await screen.findByText("No logs yet")).toBeTruthy();
    expect(screen.queryByRole("log")).toBeNull();
  });

  it("switches to another launch", async () => {
    mount();
    await view();
    fireEvent.click(screen.getByRole("button", { name: /Launch/ }));
    const options = await screen.findAllByRole("option");
    expect(options).toHaveLength(2);
    vi.mocked(readGameLog).mockResolvedValue({ text: "older line\n", truncated: false });
    fireEvent.click(options[1] as HTMLElement);
    await waitFor(() => expect(readGameLog).toHaveBeenCalledWith("i1", "1000.log"));
    await waitFor(async () => expect(lineTexts(await view())).toEqual(["older line"]));
  });

  it("filters by level and by search text", async () => {
    mount();
    const log = await view();
    await waitFor(() => expect(lineTexts(log)).toHaveLength(4));
    fireEvent.click(screen.getByRole("radio", { name: "Errors" }));
    await waitFor(() => expect(lineTexts(log)).toHaveLength(2));
    fireEvent.click(screen.getByRole("radio", { name: "All" }));
    fireEvent.change(screen.getByRole("searchbox", { name: "Search the log" }), { target: { value: "slow" } });
    await waitFor(() => expect(lineTexts(log)).toEqual(["[12:00:01 UTC] [Render thread/WARN]: Slow frame"]));
    expect(screen.getByText("1 / 4")).toBeTruthy();
  });

  it("copies the shown lines", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    mount();
    await waitFor(async () => expect(lineTexts(await view())).toHaveLength(4));
    fireEvent.click(screen.getByRole("radio", { name: "Warnings+" }));
    fireEvent.click(screen.getByRole("button", { name: "Copy the shown lines" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        "[12:00:01 UTC] [Render thread/WARN]: Slow frame\n[12:00:02 UTC] [Render thread/ERROR]: Crash\n\tat net.minecraft.Foo.bar(Foo.java:1)",
      ),
    );
    expect((await screen.findAllByText("Copied the log")).length).toBeGreaterThan(0);
  });

  it("opens the logs folder", async () => {
    vi.mocked(openInstanceFolder).mockResolvedValue(null);
    mount();
    await view();
    fireEvent.click(screen.getByRole("button", { name: "Open the logs folder" }));
    expect(openInstanceFolder).toHaveBeenCalledWith("i1", "logs");
  });

  it("renders only the last lines of a very long log, and the count shows how many there are", async () => {
    const many = Array.from({ length: MAX_RENDERED_LINES + 5 }, (_, i) => `line ${i}`).join("\n");
    vi.mocked(readGameLog).mockResolvedValue({ text: many, truncated: true });
    mount();
    const log = await view();
    await waitFor(() => expect(log.children).toHaveLength(MAX_RENDERED_LINES));
    expect(log.lastElementChild?.textContent).toBe(`line ${MAX_RENDERED_LINES + 4}`);
    expect(log.firstElementChild?.textContent).toBe("line 5");
    expect(screen.getByText(`${MAX_RENDERED_LINES + 5}+`)).toBeTruthy();
  });

  it("follows the newest log while the game runs, and not an older one", async () => {
    vi.mocked(listRunning).mockResolvedValue([session]);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mount();
    await view();
    await waitFor(() => expect(readGameLog).toHaveBeenCalledTimes(1));
    vi.mocked(readGameLog).mockResolvedValue({ text: `${sample}[12:00:03 UTC] [main/INFO]: Later\n`, truncated: false });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    await waitFor(async () => expect(lineTexts(await view())).toHaveLength(5));
  });

  it("does not poll when the game is not running", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mount();
    await view();
    await waitFor(() => expect(readGameLog).toHaveBeenCalledTimes(1));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(readGameLog).toHaveBeenCalledTimes(1);
    expect(within(await view()).queryAllByText(/Later/)).toHaveLength(0);
  });
});
