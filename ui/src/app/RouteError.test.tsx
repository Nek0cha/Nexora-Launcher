import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { createHashRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appInfo } from "../api/commands";
import i18n from "../i18n";
import { RouteError } from "./RouteError";

vi.mock("../api/commands", () => ({ appInfo: vi.fn(), listInstances: vi.fn() }));

const win = vi.hoisted(() => ({
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  isMaximized: vi.fn(async () => false),
  onResized: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));

function Broken(): never {
  throw new Error("render exploded");
}

function mount(os: string) {
  vi.mocked(appInfo).mockResolvedValue({ version: "0", build: "dev", os, dataDir: "", materialAvailable: false });
  window.history.replaceState(null, "", "/");
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createHashRouter([{ path: "/", element: <Broken />, errorElement: <RouteError /> }]);
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  // 描画の失敗は React と React Router がコンソールに出すので、テストの出力を汚さないよう黙らせる
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(appInfo).mockReset();
});

describe("RouteError", () => {
  it("shows the error and window controls on Windows", async () => {
    mount("windows");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("This screen could not be shown");
    expect(alert.textContent).toContain("render exploded");
    expect(await screen.findByRole("button", { name: "Close" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Minimize" })).toBeTruthy();
  });

  it("does not show window controls on other systems", async () => {
    mount("macos");
    await screen.findByRole("alert");
    await waitFor(() => expect(vi.mocked(appInfo)).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
  });
});
