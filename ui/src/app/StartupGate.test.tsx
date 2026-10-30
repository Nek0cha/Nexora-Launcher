import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { appInfo, getStartupStatus } from "../api/commands";
import { IpcError } from "../api/ipc";
import { StartupGate } from "./StartupGate";

vi.mock("../api/commands", () => ({ getStartupStatus: vi.fn(), appInfo: vi.fn(), listInstances: vi.fn() }));

const win = vi.hoisted(() => ({
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  isMaximized: vi.fn(async () => false),
  onResized: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));

const info = (os: string) => ({ version: "0", build: "dev", os, dataDir: "", materialAvailable: false });

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <StartupGate>
        <p>app</p>
      </StartupGate>
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.mocked(getStartupStatus).mockReset();
  vi.mocked(appInfo).mockReset();
  vi.restoreAllMocks();
});

const failed = {
  state: "failed" as const,
  error: { code: "db.migration_failed", params: {}, detail: "bad file" },
};

describe("StartupGate", () => {
  it("renders children when ready", async () => {
    vi.mocked(getStartupStatus).mockResolvedValue({ state: "ready" });
    mount();
    expect(await screen.findByText("app")).toBeTruthy();
  });

  it("shows the startup error screen when the status reports a failure", async () => {
    vi.mocked(getStartupStatus).mockResolvedValue({
      state: "failed",
      error: { code: "db.migration_failed", params: {}, detail: "bad file" },
    });
    mount();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("db.migration_failed");
    expect(alert.textContent).toContain("Updating the database failed");
    expect(screen.queryByText("app")).toBeNull();
  });

  it("shows the startup error screen when the status cannot be fetched", async () => {
    vi.mocked(getStartupStatus).mockRejectedValue(
      new IpcError({ code: "app.unexpected", params: {}, detail: "ipc unavailable" }),
    );
    mount();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("app.unexpected");
    expect(alert.textContent).toContain("ipc unavailable");
  });

  it("shows window controls on the startup error screen on Windows", async () => {
    vi.mocked(appInfo).mockResolvedValue(info("windows"));
    vi.mocked(getStartupStatus).mockResolvedValue(failed);
    mount();
    await screen.findByRole("alert");
    expect(await screen.findByRole("button", { name: "Close" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Minimize" })).toBeTruthy();
    expect(screen.getByTestId("fallback-title-bar").getAttribute("data-tauri-drag-region")).toBe("deep");
  });

  it("does not show window controls on the startup error screen on other systems", async () => {
    vi.mocked(appInfo).mockResolvedValue(info("linux"));
    vi.mocked(getStartupStatus).mockResolvedValue(failed);
    mount();
    await screen.findByRole("alert");
    await waitFor(() => expect(vi.mocked(appInfo)).toHaveBeenCalled());
    // app_info が届くまで待ってから、出ていないことを確かめる
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
    expect(screen.queryByTestId("fallback-title-bar")).toBeNull();
  });

  it("falls back to the browser platform when app_info fails", async () => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue("Win32");
    vi.mocked(appInfo).mockRejectedValue(new IpcError({ code: "app.unexpected", params: {}, detail: null }));
    vi.mocked(getStartupStatus).mockResolvedValue(failed);
    mount();
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "Close" })).toBeTruthy();
  });

  it("shows window controls while starting on Windows", async () => {
    vi.mocked(appInfo).mockResolvedValue(info("windows"));
    vi.mocked(getStartupStatus).mockResolvedValue({ state: "starting" });
    mount();
    expect(await screen.findByRole("button", { name: "Close" })).toBeTruthy();
    expect(screen.queryByText("app")).toBeNull();
  });

  describe("startup screen", () => {
    beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: false }));
    afterEach(() => vi.useRealTimers());
    const tick = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

    it("shows the logo screen only after 150ms of starting", async () => {
      vi.mocked(getStartupStatus).mockResolvedValue({ state: "starting" });
      mount();
      await tick(100);
      expect(screen.queryByTestId("startup-splash")).toBeNull();
      await tick(60);
      expect(screen.getByTestId("startup-splash")).toBeTruthy();
      expect(screen.getByText("Nexora")).toBeTruthy();
      expect(screen.queryByText("app")).toBeNull();
    });

    it("never shows the logo screen when ready within 150ms", async () => {
      vi.mocked(getStartupStatus).mockResolvedValue({ state: "ready" });
      mount();
      await tick(0);
      expect(screen.getByText("app")).toBeTruthy();
      await tick(500);
      expect(screen.queryByTestId("startup-splash")).toBeNull();
    });

    it("keeps the logo screen for 300ms before showing the children", async () => {
      let status: { state: "starting" } | { state: "ready" } = { state: "starting" };
      vi.mocked(getStartupStatus).mockImplementation(async () => status);
      mount();
      await tick(160);
      expect(screen.getByTestId("startup-splash")).toBeTruthy();
      status = { state: "ready" };
      await tick(110);
      expect(screen.getByTestId("startup-splash")).toBeTruthy();
      expect(screen.queryByText("app")).toBeNull();
      await tick(400);
      expect(screen.getByText("app")).toBeTruthy();
    });
  });
});
