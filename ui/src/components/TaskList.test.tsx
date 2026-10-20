import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cancelTask } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { TaskList } from "./TaskList";

vi.mock("../api/commands", () => ({ cancelTask: vi.fn() }));

const running: TaskSnapshot = {
  id: "t1", kind: "install", instanceId: null, state: "running", progress: { current: 40, total: 100 }, revision: 1,
  error: null, createdAt: 0, updatedAt: 0,
};

function mount(tasks: TaskSnapshot[]) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  return render(
    <QueryClientProvider client={client}>
      <TaskList />
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

describe("TaskList", () => {
  it("says there are no tasks when the table is empty", () => {
    mount([]);
    expect(screen.getByText("No tasks")).toBeTruthy();
  });

  it("keeps finished tasks after the running ones", () => {
    mount([{ ...running, id: "t0", state: "succeeded", updatedAt: 5 }, running]);
    const labels = screen.getAllByRole("progressbar").map((bar) => bar.textContent);
    expect(labels[0]).toContain("40%");
    expect(labels[1]).toContain("Done");
  });

  it("shows the task progress", () => {
    mount([running]);
    expect(screen.getByRole("progressbar", { name: "Install" }).getAttribute("aria-valuenow")).toBe("40");
  });

  it("labels a login task in both languages", async () => {
    mount([{ ...running, kind: "login" }]);
    expect(screen.getByRole("progressbar", { name: "Sign in" })).toBeTruthy();
    await act(async () => {
      await i18n.changeLanguage("ja");
    });
    expect(screen.getByRole("progressbar", { name: "ログイン" })).toBeTruthy();
  });

  it("cancels a task by id", async () => {
    vi.mocked(cancelTask).mockResolvedValue(null);
    mount([running]);
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
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    });
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The task was not found");
    expect(document.body.textContent).not.toContain("SECRET");
  });
});
