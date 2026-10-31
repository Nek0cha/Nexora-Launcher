import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { inspectMrpack } from "../../api/commands";
import { type DragDropHandlers, onFilesDropped } from "../../api/dragDrop";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { closeMrpackImport } from "../../features/mrpack/useMrpackImport";
import { TASKS_KEY } from "../../features/tasks/useTasks";
import i18n from "../../i18n";
import { ToastRegion } from "../toast/ToastRegion";
import { clearToasts } from "../toast/useToast";
import { MrpackDropHost } from "./MrpackDropHost";

vi.mock("../../api/commands", () => ({
  inspectMrpack: vi.fn(),
  importMrpack: vi.fn(),
  listInstances: vi.fn(),
}));
vi.mock("../../api/dragDrop", () => ({ onFilesDropped: vi.fn() }));

let handlers: DragDropHandlers;
let client: QueryClient;

const task = (patch: Partial<TaskSnapshot> = {}): TaskSnapshot => ({
  id: "t1", kind: "mrpack-import", instanceId: "i1", state: "running", phase: null,
  progress: { current: 0, total: 0 }, revision: 1, error: null, createdAt: 0, updatedAt: 0, ...patch,
});

function mount() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, {});
  render(
    <QueryClientProvider client={client}>
      <MrpackDropHost />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onFilesDropped).mockImplementation(async (h) => {
    handlers = h;
    return () => {};
  });
  vi.mocked(inspectMrpack).mockResolvedValue({
    name: "Dropped Pack", versionId: "1", mcVersion: "1.21.1", loader: "vanilla", loaderVersion: null, fileCount: 0,
  });
});

afterEach(() => {
  act(() => {
    closeMrpackImport();
    clearToasts();
  });
  cleanup();
  vi.resetAllMocks();
});

describe("MrpackDropHost", () => {
  it("opens the import dialog for a dropped .mrpack, whatever its case", async () => {
    mount();
    await waitFor(() => expect(handlers).toBeTruthy());

    act(() => handlers.drop(["C:\\readme.txt", "C:\\packs\\Dropped.MRPACK"]));

    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(inspectMrpack).toHaveBeenCalledWith("C:\\packs\\Dropped.MRPACK");
  });

  it("ignores drops without a .mrpack", async () => {
    mount();
    await waitFor(() => expect(handlers).toBeTruthy());

    act(() => handlers.drop(["C:\\a.zip", "C:\\skin.png"]));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(inspectMrpack).not.toHaveBeenCalled();
  });

  it("tells the user when an import task fails, once", async () => {
    mount();
    await waitFor(() => expect(handlers).toBeTruthy());
    // 取得した表の変化は次のティックで届くため、実行中の状態が画面に届いてから、失敗に変える
    await act(async () => {
      client.setQueryData(TASKS_KEY, { t1: task() });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    await act(async () => {
      client.setQueryData(TASKS_KEY, {
        t1: task({
          state: "failed",
          revision: 2,
          error: { code: "net.offline", params: {}, detail: null },
        }),
      });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(await screen.findAllByText("Can't connect to the internet")).toHaveLength(1);
  });

  it("stays quiet for tasks that already ended before it saw them", async () => {
    mount();
    await waitFor(() => expect(handlers).toBeTruthy());

    act(() =>
      client.setQueryData(TASKS_KEY, {
        t1: task({ state: "failed", error: { code: "net.offline", params: {}, detail: null } }),
      }),
    );

    expect(screen.queryByText("Can't connect to the internet")).toBeNull();
  });
});
