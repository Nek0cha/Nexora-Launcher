import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createHashRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances } from "../../api/commands";
import type { Instance } from "../../bindings/Instance";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { TASKS_KEY } from "../../features/tasks/useTasks";
import i18n from "../../i18n";
import { AppBar, isDragRegionEvent } from "./AppBar";

vi.mock("../../api/commands", () => ({ listInstances: vi.fn(), cancelTask: vi.fn() }));

const win = vi.hoisted(() => ({
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  isMaximized: vi.fn(),
  onResized: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));

let resized: (() => void) | undefined;

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

const task = (patch: Partial<TaskSnapshot> = {}): TaskSnapshot => ({
  id: "t1", kind: "install", instanceId: null, state: "running", phase: null, progress: { current: 40, total: 100 }, revision: 1,
  error: null, createdAt: 0, updatedAt: 0, ...patch,
});

function mount({ showWindowControls = true, tasks = [] as TaskSnapshot[] } = {}) {
  window.history.replaceState(null, "", "/");
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  const router = createHashRouter([
    { path: "*", element: <AppBar showWindowControls={showWindowControls} slot={<span>slot content</span>} /> },
  ]);
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router, client };
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  resized = undefined;
  vi.mocked(listInstances).mockResolvedValue([instance]);
  win.minimize.mockResolvedValue(undefined);
  win.toggleMaximize.mockResolvedValue(undefined);
  win.close.mockResolvedValue(undefined);
  win.isMaximized.mockResolvedValue(false);
  win.onResized.mockImplementation(async (handler: () => void) => {
    resized = handler;
    return () => {};
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AppBar", () => {
  it("shows_the_current_page_name", async () => {
    const { router } = mount();
    expect(screen.getByTestId("page-name").textContent).toBe("Home");
    await act(async () => {
      await router.navigate("/accounts");
    });
    expect(screen.getByTestId("page-name").textContent).toBe("Accounts");
    await act(async () => {
      await router.navigate("/instances/i1");
    });
    await waitFor(() => expect(screen.getByTestId("page-name").textContent).toContain("Survival"));
    expect(screen.getByRole("link", { name: "Instances" })).toBeTruthy();
    await act(async () => {
      await i18n.changeLanguage("ja");
      await router.navigate("/settings");
    });
    expect(screen.getByTestId("page-name").textContent).toBe("設定");
    await act(async () => {
      await router.navigate("/sync");
    });
    expect(screen.getByTestId("page-name").textContent).toBe("共通設定");
  });

  it("aligns_the_logo_and_name_with_the_sidebar_icon_and_label", () => {
    mount();
    // サイドバーのナビの余白 12px + 項目の余白 12px = 24px（pl-6）
    expect(screen.getByTestId("app-bar").className).toContain("pl-6");
    // ロゴはアイコンと同じ size-5、次の文字は gap 10px（gap-2.5）でラベルの左端に揃う
    const brand = screen.getByTestId("brand");
    expect(brand.className).toContain("gap-2.5");
    expect(brand.querySelector("svg")?.getAttribute("class")).toContain("size-5");
  });

  it("back_and_forward_disabled_at_history_ends", async () => {
    const { router } = mount();
    const back = () => screen.getByRole("button", { name: "Back" });
    const forward = () => screen.getByRole("button", { name: "Forward" });
    expect(back().hasAttribute("disabled")).toBe(true);
    expect(forward().hasAttribute("disabled")).toBe(true);

    await act(async () => {
      await router.navigate("/instances");
    });
    expect(back().hasAttribute("disabled")).toBe(false);
    expect(forward().hasAttribute("disabled")).toBe(true);

    fireEvent.click(back());
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/");
      expect(back().hasAttribute("disabled")).toBe(true);
      expect(forward().hasAttribute("disabled")).toBe(false);
    });

    fireEvent.click(forward());
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/instances");
      expect(back().hasAttribute("disabled")).toBe(false);
      expect(forward().hasAttribute("disabled")).toBe(true);
    });
  });

  it("window_controls_only_when_requested", async () => {
    mount({ showWindowControls: false });
    expect(screen.queryByRole("button", { name: "Minimize" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Maximize" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
    // ウィンドウボタンが無くても、差し込みの場所とタスクのボタンは出す
    expect(screen.getByText("slot content")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Tasks" })).toBeTruthy();
    cleanup();

    mount({ showWindowControls: true });
    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    expect(win.minimize).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(win.close).toHaveBeenCalledTimes(1);
  });

  it("maximize_button_changes_name_when_maximized", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Maximize" }));
    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("maximize-icon")).toBeTruthy();

    // 最大化はサイズの変化として届き、問い合わせ直して見た目と名前を変える
    win.isMaximized.mockResolvedValue(true);
    await waitFor(() => expect(resized).toBeDefined());
    await act(async () => {
      resized?.();
    });
    expect(await screen.findByRole("button", { name: "Restore down" })).toBeTruthy();
    expect(screen.getByTestId("restore-icon")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Maximize" })).toBeNull();

    win.isMaximized.mockResolvedValue(false);
    await act(async () => {
      resized?.();
    });
    expect(await screen.findByRole("button", { name: "Maximize" })).toBeTruthy();
  });

  it("double_click_on_drag_region_toggles_maximize", async () => {
    mount();
    const region = screen.getByTestId("drag-region");
    const bar = screen.getByTestId("app-bar");
    expect(bar.getAttribute("data-tauri-drag-region")).toBe("deep");

    // 1 回目の mousedown は移動の開始として Tauri に任せる
    fireEvent.mouseDown(region, { button: 0, detail: 1 });
    expect(win.toggleMaximize).not.toHaveBeenCalled();

    // 2 回目で切り替え、document（Tauri の処理）には届けない
    const reachedDocument = vi.fn();
    document.addEventListener("mousedown", reachedDocument);
    fireEvent.mouseDown(region, { button: 0, detail: 2 });
    document.removeEventListener("mousedown", reachedDocument);
    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(reachedDocument).not.toHaveBeenCalled();

    // ボタンの上のダブルクリックは、最大化に使わない
    fireEvent.mouseDown(screen.getByRole("button", { name: "Back" }), { button: 0, detail: 2 });
    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
  });

  it("double_click_inside_the_task_popover_does_not_toggle_maximize", async () => {
    mount({ tasks: [task()] });
    await act(async () => {
      fireEvent.click(await screen.findByRole("button", { name: "Tasks, 1 running" }));
    });
    const dialog = await screen.findByRole("dialog", { name: "Tasks" });
    // ポップアップはポータルで body に出ており、バーの DOM の中には無い
    expect(screen.getByTestId("app-bar").contains(dialog)).toBe(false);
    const text = screen.getByText("Install");
    const down = fireEvent.mouseDown(text, { button: 0, detail: 2 });
    fireEvent.mouseUp(text, { button: 0, detail: 2 });
    expect(win.toggleMaximize).not.toHaveBeenCalled();
    // 文字の選択を妨げない
    expect(down).toBe(true);
  });

  it("drag_region_rules_match_tauri", () => {
    const region = document.createElement("div");
    region.setAttribute("data-tauri-drag-region", "deep");
    const make = (tag: string, attrs: Record<string, string> = {}) => {
      const el = document.createElement(tag);
      for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
      return el;
    };
    expect(isDragRegionEvent([make("span"), region])).toBe(true);
    for (const el of [
      make("summary"),
      make("div", { role: "tab" }),
      make("div", { role: "menuitem" }),
      make("div", { role: "switch" }),
      make("div", { contenteditable: "true" }),
      make("div", { tabindex: "0" }),
      make("div", { "data-tauri-drag-region": "false" }),
    ]) {
      expect(isDragRegionEvent([el, region])).toBe(false);
    }
    expect(isDragRegionEvent([make("div", { tabindex: "-1" }), region])).toBe(true);
    expect(isDragRegionEvent([make("span"), document.body])).toBe(false);
  });

  it("task_button_shows_progress_only_when_tasks_are_active_and_opens_the_list", async () => {
    const { client } = mount({ tasks: [task({ state: "succeeded" })] });
    const idle = screen.getByRole("button", { name: "Tasks" });
    expect(screen.queryByTestId("task-ring")).toBeNull();

    // 終わったタスクだけでも、押すと一覧を開ける
    await act(async () => {
      fireEvent.click(idle);
    });
    expect(await screen.findByRole("dialog", { name: "Tasks" })).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "Install" }).textContent).toContain("Done");
    await act(async () => {
      fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    });
    await waitFor(() => expect(screen.queryByRole("dialog", { hidden: true })).toBeNull());

    await act(async () => {
      client.setQueryData(TASKS_KEY, { t2: task({ id: "t2" }) });
    });
    const busy = await screen.findByRole("button", { name: "Tasks, 1 running" });
    expect(screen.getByTestId("task-ring")).toBeTruthy();
    await act(async () => {
      fireEvent.click(busy);
    });
    expect(await screen.findByRole("dialog", { name: "Tasks" })).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "Install" }).getAttribute("aria-valuenow")).toBe("40");
  });
});
