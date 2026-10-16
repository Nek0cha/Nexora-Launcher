import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appInfo, getDevOfflineUsername, setDevOfflineUsername } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { AppInfo } from "../bindings/AppInfo";
import i18n from "../i18n";
import Accounts from "./Accounts";

vi.mock("../api/commands", () => ({
  appInfo: vi.fn(),
  getDevOfflineUsername: vi.fn(),
  setDevOfflineUsername: vi.fn(),
}));

const info = (build: string): AppInfo => ({ version: "0.1.0", build, os: "windows", dataDir: "d", materialAvailable: false });

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Accounts />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const field = () => screen.findByRole("textbox", { name: "Development offline account" }) as Promise<HTMLInputElement>;

describe("Accounts", () => {
  it("has no offline account in release builds", async () => {
    vi.mocked(appInfo).mockResolvedValue(info("release"));
    mount();
    expect(await screen.findByText("Signing in with a Microsoft account will arrive in a future version.")).toBeTruthy();
    await act(async () => {});
    expect(screen.queryByText("Development offline account")).toBeNull();
    expect(getDevOfflineUsername).not.toHaveBeenCalled();
  });

  it("loads and saves the offline name in development builds", async () => {
    vi.mocked(appInfo).mockResolvedValue(info("dev"));
    vi.mocked(getDevOfflineUsername).mockResolvedValue("Player");
    vi.mocked(setDevOfflineUsername).mockResolvedValue("Steve_1");
    mount();
    const input = await field();
    expect(input.value).toBe("Player");
    expect(input.maxLength).toBe(16);
    fireEvent.change(input, { target: { value: "Steve_1" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(setDevOfflineUsername).toHaveBeenCalledWith("Steve_1");
    expect(await screen.findByText("Saved")).toBeTruthy();
  });

  it("shows an invalid name error from the backend", async () => {
    vi.mocked(appInfo).mockResolvedValue(info("dev"));
    vi.mocked(getDevOfflineUsername).mockResolvedValue("Player");
    vi.mocked(setDevOfflineUsername).mockRejectedValue(
      new IpcError({ code: "settings.invalid_value", params: {}, detail: null }),
    );
    mount();
    const input = await field();
    fireEvent.change(input, { target: { value: "bad name!" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(await screen.findByText("The setting value is invalid")).toBeTruthy();
    expect(screen.queryByText("Saved")).toBeNull();
    expect(input.value).toBe("bad name!");
  });
});
