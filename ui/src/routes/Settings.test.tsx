import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getSettings, setSetting } from "../api/commands";
import { IpcError } from "../api/ipc";
import i18n from "../i18n";
import Settings from "./Settings";

vi.mock("../api/commands", () => ({ getSettings: vi.fn(), setSetting: vi.fn(), appInfo: vi.fn() }));

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Settings />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("Settings route", () => {
  it("does not repeat the label under the download count", async () => {
    vi.mocked(getSettings).mockResolvedValue({
      theme: "dark",
      language: "en",
      maxConcurrentDownloads: 8,
      defaultMemoryMb: 2048,
      lastAddedInstance: null,
    });
    mount();
    await screen.findByText("Concurrent downloads");
    expect(screen.queryByText("How many files are downloaded in parallel.")).toBeNull();
  });

  it("changes the default memory used by instances that follow the global setting", async () => {
    vi.mocked(getSettings).mockResolvedValue({
      theme: "dark",
      language: "en",
      maxConcurrentDownloads: 8,
      defaultMemoryMb: 2048,
      lastAddedInstance: null,
    });
    vi.mocked(setSetting).mockResolvedValue({
      theme: "dark",
      language: "en",
      maxConcurrentDownloads: 8,
      defaultMemoryMb: 8192,
      lastAddedInstance: null,
    });
    mount();
    const input = await screen.findByRole("textbox", { name: "Default memory (MB)" });
    expect((input as HTMLInputElement).value).toBe("2,048");
    fireEvent.change(input, { target: { value: "8192" } });
    fireEvent.blur(input);
    await waitFor(() => expect(setSetting).toHaveBeenCalledWith({ key: "defaultMemoryMb", value: 8192 }));
  });

  it("shows an error instead of a blank screen when loading fails", async () => {
    vi.mocked(getSettings).mockRejectedValue(new IpcError({ code: "db.failed", params: {}, detail: "secret path" }));
    mount();
    expect(await screen.findByText("A database operation failed")).toBeTruthy();
    expect(screen.queryByText("secret path")).toBeNull();
  });

  it("shows the translated error when a write fails", async () => {
    vi.mocked(getSettings).mockResolvedValue({
      theme: "dark",
      language: "en",
      maxConcurrentDownloads: 8,
      defaultMemoryMb: 2048,
      lastAddedInstance: null,
    });
    vi.mocked(setSetting).mockRejectedValue(new IpcError({ code: "settings.invalid_value", params: {}, detail: "raw" }));
    mount();
    const buttons = await screen.findAllByRole("button", { name: /increase|increment/i });
    fireEvent.click(buttons[0]!);
    expect(await screen.findByText("The setting value is invalid")).toBeTruthy();
    expect(screen.queryByText("raw")).toBeNull();
  });
});
