import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appInfo } from "../api/commands";
import { openExternal } from "../api/opener";
import type { AppInfo } from "../bindings/AppInfo";
import { checkUpdate } from "../api/updater";
import { resetUpdateStore } from "../features/update/updateStore";
import { RELEASES_URL, REPO_URL } from "../features/update/repo";
import i18n from "../i18n";
import { AboutRows } from "./AboutSection";

vi.mock("../api/commands", () => ({ appInfo: vi.fn() }));
vi.mock("../api/opener", () => ({ openExternal: vi.fn() }));
vi.mock("../api/updater", () => ({ checkUpdate: vi.fn(), restartApp: vi.fn() }));
vi.mock("../features/play/useGameSync", () => ({ useRunning: () => [] }));

const info = (over: Partial<AppInfo> = {}): AppInfo => ({
  version: "0.1.0",
  build: "release",
  os: "windows",
  dataDir: "",
  materialAvailable: false,
  selfUpdate: true,
  ...over,
});

function mount(over: Partial<AppInfo> = {}) {
  vi.mocked(appInfo).mockResolvedValue(info(over));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AboutRows />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  resetUpdateStore();
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("AboutRows", () => {
  it("shows the version and the license", async () => {
    mount();
    expect(await screen.findByText("0.1.0")).toBeTruthy();
    expect(screen.getByText("Apache-2.0")).toBeTruthy();
  });

  it("the repository link opens in the browser", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "GitHub" }));
    expect(openExternal).toHaveBeenCalledWith(REPO_URL);
  });

  it("a manual check shows up to date", async () => {
    vi.mocked(checkUpdate).mockResolvedValue(null);
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Up to date")).toBeTruthy();
  });

  it("a manual check shows a failure", async () => {
    vi.mocked(checkUpdate).mockRejectedValue(new Error("offline"));
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Could not check")).toBeTruthy();
  });

  it("a manual check shows the available version with an install button", async () => {
    vi.mocked(checkUpdate).mockResolvedValue({ version: "0.1.1", install: async () => {} });
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("0.1.1")).toBeTruthy();
    const install = screen.getByRole("button", { name: "Update and restart" });
    expect(install.hasAttribute("disabled")).toBe(false);
    await act(async () => {});
  });

  it("without self update only the releases link is shown", async () => {
    mount({ selfUpdate: false });
    expect(await screen.findByText("0.1.0")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Releases" }));
    expect(openExternal).toHaveBeenCalledWith(RELEASES_URL);
  });
});
