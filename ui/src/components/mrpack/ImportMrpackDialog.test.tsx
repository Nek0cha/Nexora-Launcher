import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { importMrpack, inspectMrpack } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { MrpackInfo } from "../../bindings/MrpackInfo";
import { closeMrpackImport, openMrpackImport } from "../../features/mrpack/useMrpackImport";
import i18n from "../../i18n";
import { ImportMrpackDialog } from "./ImportMrpackDialog";

vi.mock("../../api/commands", () => ({
  inspectMrpack: vi.fn(),
  importMrpack: vi.fn(),
  listInstances: vi.fn(),
}));

const info = (patch: Partial<MrpackInfo> = {}): MrpackInfo => ({
  name: "Cool Pack",
  versionId: "1.2.0",
  mcVersion: "1.21.1",
  loader: "fabric",
  loaderVersion: "0.16.10",
  fileCount: 12,
  ...patch,
});

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ImportMrpackDialog />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  act(() => closeMrpackImport());
  cleanup();
  vi.resetAllMocks();
});

const nameInput = () => screen.findByRole("textbox", { name: "Name" }) as Promise<HTMLInputElement>;
const submit = () => screen.getByRole("button", { name: "Import" });

describe("ImportMrpackDialog", () => {
  it("shows nothing until a pack is chosen", () => {
    mount();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows the pack summary and uses the pack name as the default name", async () => {
    vi.mocked(inspectMrpack).mockResolvedValue(info());
    mount();
    act(() => openMrpackImport("C:\\packs\\cool.mrpack"));

    expect((await nameInput()).value).toBe("Cool Pack");
    expect(inspectMrpack).toHaveBeenCalledWith("C:\\packs\\cool.mrpack");
    expect(await screen.findByText("1.21.1")).toBeTruthy();
    expect(screen.getByText("Fabric 0.16.10")).toBeTruthy();
    expect(screen.getByText("12")).toBeTruthy();
  });

  it("imports with the typed name", async () => {
    vi.mocked(inspectMrpack).mockResolvedValue(info());
    vi.mocked(importMrpack).mockResolvedValue({ instance: {} as never, taskId: "t1" });
    mount();
    act(() => openMrpackImport("C:\\packs\\cool.mrpack"));
    const input = await nameInput();

    fireEvent.change(input, { target: { value: "  My copy " } });
    await act(async () => {
      fireEvent.click(submit());
    });

    await waitFor(() => expect(importMrpack).toHaveBeenCalledWith("C:\\packs\\cool.mrpack", "My copy"));
    // 取り込みを始めたらダイアログは閉じる
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("does not allow a blank name", async () => {
    vi.mocked(inspectMrpack).mockResolvedValue(info());
    mount();
    act(() => openMrpackImport("C:\\packs\\cool.mrpack"));
    const input = await nameInput();

    fireEvent.change(input, { target: { value: "   " } });

    await waitFor(() => expect(submit().hasAttribute("disabled")).toBe(true));
    expect(importMrpack).not.toHaveBeenCalled();
  });

  it("shows why a pack cannot be read and disables importing", async () => {
    vi.mocked(inspectMrpack).mockRejectedValue(
      new IpcError({ code: "mrpack.unsupported", params: { reason: "formatVersion 2" }, detail: null }),
    );
    mount();
    act(() => openMrpackImport("C:\\packs\\bad.mrpack"));

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(submit().hasAttribute("disabled")).toBe(true);
  });

  it("shows the failure of the import call itself", async () => {
    vi.mocked(inspectMrpack).mockResolvedValue(info());
    vi.mocked(importMrpack).mockRejectedValue(
      new IpcError({ code: "mrpack.unsafe_path", params: { path: "../x" }, detail: null }),
    );
    mount();
    act(() => openMrpackImport("C:\\packs\\cool.mrpack"));
    await nameInput();

    await act(async () => {
      fireEvent.click(submit());
    });

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByRole("dialog")).not.toBeNull();
  });
});
