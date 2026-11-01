import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { findImportDuplicates, importFromLauncher, listImportInstances } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import { IpcError } from "../../api/ipc";
import type { SourceInstance } from "../../bindings/SourceInstance";
import { closeLauncherImport, openLauncherImport } from "../../features/import/useLauncherImport";
import i18n from "../../i18n";
import { ImportLauncherDialog } from "./ImportLauncherDialog";

vi.mock("../../api/commands", () => ({
  listImportInstances: vi.fn(),
  findImportDuplicates: vi.fn(),
  importFromLauncher: vi.fn(),
  listInstances: vi.fn(),
}));
vi.mock("../../api/dialog", () => ({ pickFolder: vi.fn() }));

const row = (patch: Partial<SourceInstance> = {}): SourceInstance => ({
  id: "a",
  name: "Alpha",
  mcVersion: "1.21.1",
  loader: "fabric",
  loaderVersion: "0.16.10",
  available: ["mods", "saves"],
  unsupported: null,
  ...patch,
});

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ImportLauncherDialog />
    </QueryClientProvider>,
  );
  act(() => openLauncherImport());
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  act(() => closeLauncherImport());
  cleanup();
  vi.resetAllMocks();
});

const check = (name: string) => screen.findByRole("checkbox", { name });
const importButton = () => screen.getByRole("button", { name: /^Import( \d+)?$/ });

describe("ImportLauncherDialog", () => {
  it("imports only the checked rows, with the kinds left checked", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row(), row({ id: "b", name: "Bravo" })]);
    vi.mocked(findImportDuplicates).mockResolvedValue([]);
    vi.mocked(importFromLauncher).mockResolvedValue([{ instance: {} as never, taskId: "t1" }]);
    mount();

    fireEvent.click(await check("Alpha"));
    fireEvent.click(await check("Worlds"));
    fireEvent.click(screen.getByRole("button", { name: "Import 1" }));

    await waitFor(() =>
      expect(importFromLauncher).toHaveBeenCalledWith(
        "modrinth_app",
        null,
        [{ id: "a", kinds: ["mods"] }],
        false,
      ),
    );
    expect(findImportDuplicates).toHaveBeenCalledWith("modrinth_app", null, ["a"]);
  });

  it("shows the kind checkboxes only for a checked row and only for kinds that exist", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row()]);
    mount();

    await check("Alpha");
    expect(screen.queryByRole("checkbox", { name: "Mods" })).toBeNull();
    fireEvent.click(await check("Alpha"));
    expect(await check("Mods")).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: "Worlds" })).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: "Screenshots" })).toBeNull();
  });

  it("cannot check a row that is unsupported, and says why", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row({ unsupported: "the loader is not supported" })]);
    mount();

    const box = (await check("Alpha")) as HTMLInputElement;
    expect(box.disabled).toBe(true);
    expect(screen.getByText("the loader is not supported")).toBeTruthy();
  });

  it("stops on a duplicate, marks the row, and goes on only when told to", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row()]);
    vi.mocked(findImportDuplicates).mockResolvedValue(["a"]);
    vi.mocked(importFromLauncher).mockResolvedValue([{ instance: {} as never, taskId: "t1" }]);
    mount();

    fireEvent.click(await check("Alpha"));
    fireEvent.click(screen.getByRole("button", { name: "Import 1" }));

    expect(await screen.findByText("Already added")).toBeTruthy();
    expect(importFromLauncher).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Import anyway" }));
    await waitFor(() =>
      expect(importFromLauncher).toHaveBeenCalledWith(
        "modrinth_app",
        null,
        [{ id: "a", kinds: ["mods", "saves"] }],
        true,
      ),
    );
  });

  it("asks for the folder when the launcher is not found, then lists from it", async () => {
    vi.mocked(listImportInstances)
      .mockRejectedValueOnce(new IpcError({ code: "import.unavailable", params: { reason: "x" }, detail: null }))
      .mockResolvedValue([row()]);
    vi.mocked(pickFolder).mockResolvedValue("D:\\Apps\\ModrinthApp");
    mount();

    fireEvent.click(await screen.findByRole("button", { name: "Choose folder" }));

    expect(await check("Alpha")).toBeTruthy();
    expect(listImportInstances).toHaveBeenLastCalledWith("modrinth_app", "D:\\Apps\\ModrinthApp");
  });

  it("lists the instances of the launcher chosen at the top and imports from it", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row()]);
    vi.mocked(findImportDuplicates).mockResolvedValue([]);
    vi.mocked(importFromLauncher).mockResolvedValue([{ instance: {} as never, taskId: "t1" }]);
    mount();

    await check("Alpha");
    fireEvent.click(screen.getByRole("radio", { name: "Prism Launcher" }));
    await waitFor(() => expect(listImportInstances).toHaveBeenLastCalledWith("prism_launcher", null));
    fireEvent.click(await check("Alpha"));
    fireEvent.click(screen.getByRole("button", { name: "Import 1" }));

    await waitFor(() =>
      expect(importFromLauncher).toHaveBeenCalledWith(
        "prism_launcher",
        null,
        [{ id: "a", kinds: ["mods", "saves"] }],
        false,
      ),
    );
    expect(findImportDuplicates).toHaveBeenCalledWith("prism_launcher", null, ["a"]);
  });

  it("clears the selection when another launcher is chosen", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row()]);
    mount();

    fireEvent.click(await check("Alpha"));
    expect(await check("Mods")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "CurseForge" }));

    await waitFor(() => expect(listImportInstances).toHaveBeenLastCalledWith("curse_forge", null));
    expect(screen.queryByRole("checkbox", { name: "Mods" })).toBeNull();
  });

  it("shows why the launcher data could not be read, next to the folder button", async () => {
    vi.mocked(listImportInstances).mockRejectedValue(
      new IpcError({ code: "import.unavailable", params: { reason: "no such column: name" }, detail: null }),
    );
    mount();

    expect(await screen.findByRole("button", { name: "Choose folder" })).toBeTruthy();
    expect(screen.getByText(/no such column: name/)).toBeTruthy();
  });

  it("offers the settings as one checkbox and sends it", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row({ available: ["mods", "settings"] })]);
    vi.mocked(findImportDuplicates).mockResolvedValue([]);
    vi.mocked(importFromLauncher).mockResolvedValue([{ instance: {} as never, taskId: "t1" }]);
    mount();

    fireEvent.click(await check("Alpha"));
    expect(await check("Settings")).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: "Server list" })).toBeNull();
    fireEvent.click(await check("Mods"));
    fireEvent.click(screen.getByRole("button", { name: "Import 1" }));

    await waitFor(() =>
      expect(importFromLauncher).toHaveBeenCalledWith(
        "modrinth_app",
        null,
        [{ id: "a", kinds: ["settings"] }],
        false,
      ),
    );
  });

  it("handles a folder whose name is also an object property", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row({ id: "constructor", name: "Odd" })]);
    mount();

    const box = (await check("Odd")) as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    expect(await check("Mods")).toBeTruthy();
  });

  it("cannot import until a row is checked", async () => {
    vi.mocked(listImportInstances).mockResolvedValue([row()]);
    mount();

    await check("Alpha");
    expect((importButton() as HTMLButtonElement).disabled).toBe(true);
  });
});
