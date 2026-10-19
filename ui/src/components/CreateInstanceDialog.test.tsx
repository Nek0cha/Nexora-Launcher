import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createInstance, listVersions } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { CreatedInstance } from "../bindings/CreatedInstance";
import { INSTANCES_KEY } from "../features/instances/useInstances";
import i18n from "../i18n";
import { CreateInstanceDialog } from "./CreateInstanceDialog";

vi.mock("../api/commands", () => ({
  listVersions: vi.fn(),
  createInstance: vi.fn(),
  listInstances: vi.fn().mockResolvedValue([]),
}));

const VERSIONS = [
  { id: "1.21.5", releaseTime: "2025-03-25T00:00:00Z" },
  { id: "1.8.9", releaseTime: "2015-12-09T00:00:00Z" },
];

const created: CreatedInstance = {
  instance: {
    id: "i1", name: "1.21.5", dirName: "1.21.5", mcVersion: "1.21.5", state: "installing",
    createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
    memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  },
  taskId: "t1",
};

let client: QueryClient;

function mount() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(INSTANCES_KEY, []);
  render(
    <QueryClientProvider client={client}>
      <CreateInstanceDialog trigger="quiet" />
    </QueryClientProvider>,
  );
}

async function open() {
  fireEvent.click(screen.getByRole("button", { name: "Create instance" }));
  return screen.findByRole("dialog");
}

const nameInput = (dialog: HTMLElement) => within(dialog).getByRole("textbox", { name: "Name" }) as HTMLInputElement;

async function pickVersion(dialog: HTMLElement, id: string) {
  fireEvent.click(within(dialog).getByRole("button", { name: /Version/ }));
  fireEvent.click(await screen.findByRole("option", { name: id }));
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("CreateInstanceDialog", () => {
  it("selects the newest version, names the instance after it and focuses the name", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    expect(document.activeElement).toBe(nameInput(dialog));
    expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5");
  });

  it("shows a placeholder of the same row while versions load", async () => {
    vi.mocked(listVersions).mockReturnValue(new Promise(() => {}));
    mount();
    const dialog = await open();
    expect(within(dialog).getByText("Loading versions")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Create" }).hasAttribute("disabled")).toBe(true);
  });

  it("renames with the version until the user types a name", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await pickVersion(dialog, "1.8.9");
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.8.9"));

    fireEvent.change(nameInput(dialog), { target: { value: "My World" } });
    await pickVersion(dialog, "1.21.5");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5"));
    expect(nameInput(dialog).value).toBe("My World");
  });

  it("creates the instance, refetches the list and closes", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(createInstance).mockResolvedValue(created);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    fireEvent.change(nameInput(dialog), { target: { value: "My World" } });
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Create" })));
    expect(createInstance).toHaveBeenCalledWith("My World", "1.21.5");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // 一覧を見ている画面（ホーム）があれば取り直される
    expect(client.getQueryState(INSTANCES_KEY)?.isInvalidated).toBe(true);
  });

  it("keeps the dialog open and shows the error when creating fails", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(createInstance).mockRejectedValue(new IpcError({ code: "db.failed", params: {}, detail: null }));
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Create" })));
    expect(await within(dialog).findByText("A database operation failed")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBe(dialog);
  });

  it("shows the error and retries when versions cannot be loaded", async () => {
    vi.mocked(listVersions).mockRejectedValueOnce(new IpcError({ code: "net.offline", params: {}, detail: null }));
    mount();
    const dialog = await open();
    expect(await within(dialog).findByText("Can't connect to the internet")).toBeTruthy();
    vi.mocked(listVersions).mockResolvedValueOnce(VERSIONS);
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Retry" })));
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    expect(listVersions).toHaveBeenCalledTimes(2);
  });

  it("closes with the cancel button without creating", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createInstance).not.toHaveBeenCalled();
  });
});
