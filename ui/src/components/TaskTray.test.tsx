import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cancelTask } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { TaskTray } from "./TaskTray";

vi.mock("../api/commands", () => ({ cancelTask: vi.fn() }));

const running: TaskSnapshot = {
  id: "t1", kind: "demo", instanceId: null, state: "running", progress: { current: 40, total: 100 }, revision: 1,
  error: null, createdAt: 0, updatedAt: 0,
};

function mount(tasks: TaskSnapshot[]) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  return render(
    <QueryClientProvider client={client}>
      <TaskTray />
    </QueryClientProvider>,
  );
}

async function openTray() {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: /Tasks/ }));
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("TaskTray", () => {
  it("renders nothing without running tasks", () => {
    const { container } = mount([{ ...running, state: "succeeded" }]);
    expect(container.innerHTML).toBe("");
  });

  it("shows the running count and the task progress", async () => {
    mount([running]);
    expect(screen.getByRole("button", { name: /Tasks/ }).textContent).toContain("1");
    await openTray();
    expect(screen.getByRole("progressbar", { name: "Demo task" }).getAttribute("aria-valuenow")).toBe("40");
  });

  it("cancels a task by id", async () => {
    vi.mocked(cancelTask).mockResolvedValue(null);
    mount([running]);
    await openTray();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    });
    expect(cancelTask).toHaveBeenCalledWith("t1");
  });

  it("shows a cancel failure in the row by error code, never the raw detail", async () => {
    vi.mocked(cancelTask).mockRejectedValue(
      new IpcError({ code: "task.not_found", params: {}, detail: "SECRET raw detail" }),
    );
    mount([running]);
    await openTray();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    });
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The task was not found");
    expect(document.body.textContent).not.toContain("SECRET");
  });
});
