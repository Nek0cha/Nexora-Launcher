import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances, listRunning, systemMemoryMb, updateInstance } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import { ToastRegion } from "../components/toast/ToastRegion";
import { clearToasts } from "../components/toast/useToast";
import { RUNNING_KEY } from "../features/play/useGameSync";
import i18n from "../i18n";
import InstanceDetail from "./InstanceDetail";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  updateInstance: vi.fn(),
  systemMemoryMb: vi.fn(),
}));

const instance = (patch: Partial<Instance> = {}): Instance => ({
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  ...patch,
});

function mount(list: Instance[], { id = "i1", running = [] as RunningSession[], totalMb = 8192 as number | null } = {}) {
  // null は物理メモリの取得に失敗する場合
  if (totalMb === null) vi.mocked(systemMemoryMb).mockRejectedValue(new Error("no memory"));
  else vi.mocked(systemMemoryMb).mockResolvedValue(totalMb);
  vi.mocked(listInstances).mockResolvedValue(list);
  vi.mocked(listRunning).mockResolvedValue(running);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(RUNNING_KEY, running);
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[`/instances/${id}`]}>
        <Routes>
          <Route path="/instances/:id" element={<InstanceDetail />} />
          <Route path="/instances" element={<p>list page</p>} />
        </Routes>
      </MemoryRouter>
      <ToastRegion />
    </QueryClientProvider>,
  );
}

const toasts = () => screen.getByRole("region", { name: "Notifications" });
const saveButton = () => screen.findByRole("button", { name: "Save" }) as Promise<HTMLButtonElement>;
const nameInput = () => screen.getByRole("textbox", { name: "Name" });
const jvmInput = () => screen.getByRole("textbox", { name: "JVM arguments" });

async function choose(select: RegExp, option: string) {
  fireEvent.click(screen.getByRole("button", { name: select }));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}

async function press(el: HTMLElement) {
  await act(async () => {
    fireEvent.click(el);
    await Promise.resolve();
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

describe("InstanceDetail", () => {
  it("save_is_disabled_until_changed", async () => {
    mount([instance()]);
    expect(await screen.findByRole("heading", { level: 1, name: "Survival" })).toBeTruthy();
    expect(screen.getByText("1.21.5")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Settings" })).toBeTruthy();
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    expect(screen.getByRole("link", { name: "Instances" }).getAttribute("href")).toBe("/instances");
    expect((await saveButton()).disabled).toBe(true);
    fireEvent.change(nameInput(), { target: { value: "Creative" } });
    expect((await saveButton()).disabled).toBe(false);
    fireEvent.change(nameInput(), { target: { value: "Survival" } });
    expect((await saveButton()).disabled).toBe(true);
  });

  it("saving_sends_trimmed_settings", async () => {
    const saved = instance({ name: "New", memoryMode: "manual", memoryMb: 4096 });
    vi.mocked(updateInstance).mockResolvedValue(saved);
    mount([instance()]);
    await saveButton();
    fireEvent.change(nameInput(), { target: { value: "  New  " } });
    await choose(/Memory$/, "Manual");
    expect(screen.getByRole("textbox", { name: "Maximum memory (MB)" })).toBeTruthy();
    await choose(/Java$/, "Manual");
    fireEvent.change(screen.getByRole("textbox", { name: "Java path" }), {
      target: { value: ' "C:\\Program Files\\Java\\bin\\javaw.exe" ' },
    });
    fireEvent.change(jvmInput(), { target: { value: " -Xss1M " } });
    await press(await saveButton());
    await waitFor(() => expect(updateInstance).toHaveBeenCalledTimes(1));
    expect(updateInstance).toHaveBeenCalledWith("i1", {
      name: "New",
      memoryMode: "manual",
      memoryMb: 4096,
      javaMode: "manual",
      javaPath: "C:\\Program Files\\Java\\bin\\javaw.exe",
      jvmArgs: "-Xss1M",
    });
    // 保存の成功はトーストで知らせる
    const status = await within(toasts()).findByRole("status");
    expect(status.textContent).toContain("Saved");
  });

  it("name_input_is_limited_to_64_characters", async () => {
    mount([instance()]);
    await saveButton();
    expect((nameInput() as HTMLInputElement).maxLength).toBe(64);
  });

  it("form_errors_stay_inline_and_are_not_toasts", async () => {
    mount([instance()]);
    await saveButton();
    fireEvent.change(nameInput(), { target: { value: "   " } });
    fireEvent.submit(nameInput().closest("form")!);
    expect((await saveButton()).disabled).toBe(true);
    await act(async () => { await Promise.resolve(); });
    expect(updateInstance).not.toHaveBeenCalled();
    expect(within(toasts()).queryByRole("alert")).toBeNull();
    expect(within(toasts()).queryByRole("status")).toBeNull();
  });

  it("blank_name_blocks_saving", async () => {
    mount([instance()]);
    await saveButton();
    fireEvent.change(nameInput(), { target: { value: "   " } });
    expect((await saveButton()).disabled).toBe(true);
    fireEvent.change(nameInput(), { target: { value: "Ok" } });
    expect((await saveButton()).disabled).toBe(false);
    // 手動の Java でパスが空のときも保存できない
    await choose(/Java$/, "Manual");
    expect((await saveButton()).disabled).toBe(true);
  });

  it("xmx_in_jvm_args_shows_warning", async () => {
    mount([instance()]);
    await saveButton();
    const warning = /-Xmx is ignored/;
    expect(screen.queryByText(warning)).toBeNull();
    fireEvent.change(jvmInput(), { target: { value: "-Xss1M -Xmx4G" } });
    expect(screen.getByText(warning)).toBeTruthy();
    fireEvent.change(jvmInput(), { target: { value: "-Xms1G" } });
    expect(screen.queryByText(warning)).toBeNull();
  });

  it("running_instance_shows_next_launch_notice", async () => {
    mount([instance()], { running: [{ instanceId: "i1", pid: 1, launchedAt: 0 }] });
    expect(await screen.findByText("Changes take effect the next time the game starts.")).toBeTruthy();
  });

  it("stopped_instance_has_no_next_launch_notice", async () => {
    mount([instance()]);
    await saveButton();
    expect(screen.queryByText("Changes take effect the next time the game starts.")).toBeNull();
  });

  it("unknown_id_shows_back_to_list", async () => {
    mount([instance()], { id: "missing" });
    expect(await screen.findByText("This instance doesn't exist.")).toBeTruthy();
    await press(screen.getByRole("link", { name: "Back to instances" }));
    expect(await screen.findByText("list page")).toBeTruthy();
  });

  it("save_failure_is_a_toast", async () => {
    vi.mocked(updateInstance).mockRejectedValue(
      new IpcError({ code: "instance.invalid_setting", params: {}, detail: null }),
    );
    mount([instance()]);
    await saveButton();
    fireEvent.change(nameInput(), { target: { value: "Creative" } });
    await press(await saveButton());
    const alert = await within(toasts()).findByRole("alert");
    expect(within(alert).getByText("A setting value is not valid")).toBeTruthy();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });

  it("slider_and_number_input_stay_in_sync", async () => {
    mount([instance({ memoryMode: "manual", memoryMb: 4096 })]);
    const slider = (await screen.findByRole("slider", { name: "Memory amount" })) as HTMLInputElement;
    expect(slider.value).toBe("4096");
    fireEvent.change(slider, { target: { value: "6144" } });
    expect((screen.getByRole("textbox", { name: "Maximum memory (MB)" }) as HTMLInputElement).value).toBe("6,144");
    const number = screen.getByRole("textbox", { name: "Maximum memory (MB)" });
    fireEvent.change(number, { target: { value: "2048" } });
    fireEvent.blur(number);
    expect((screen.getByRole("slider", { name: "Memory amount" }) as HTMLInputElement).value).toBe("2048");
  });

  it("value_above_slider_max_keeps_the_number_and_pins_the_slider", async () => {
    vi.mocked(updateInstance).mockImplementation(async (_id, s) => instance({ ...s }));
    mount([instance({ memoryMode: "manual", memoryMb: 32768 })]);
    const slider = (await screen.findByRole("slider", { name: "Memory amount" })) as HTMLInputElement;
    expect(slider.value).toBe("8192");
    expect((screen.getByRole("textbox", { name: "Maximum memory (MB)" }) as HTMLInputElement).value).toBe("32,768");
    // 名前だけを変えて保存しても、メモリの値は変わらない
    fireEvent.change(nameInput(), { target: { value: "Creative" } });
    await press(await saveButton());
    await waitFor(() => expect(updateInstance).toHaveBeenCalledTimes(1));
    expect(vi.mocked(updateInstance).mock.calls[0]?.[1]).toMatchObject({ memoryMode: "manual", memoryMb: 32768 });
    expect((screen.getByRole("textbox", { name: "Maximum memory (MB)" }) as HTMLInputElement).value).toBe("32,768");
  });

  it("slider_is_keyboard_operable_and_announces_value", async () => {
    mount([instance({ memoryMode: "manual", memoryMb: 4096 })]);
    const slider = (await screen.findByRole("slider", { name: "Memory amount" })) as HTMLInputElement;
    expect(slider.getAttribute("aria-valuetext")).toMatch(/4,?096\s?MB/);
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    fireEvent.change(slider, { target: { value: "4352" } });
    expect(slider.getAttribute("aria-valuetext")).toMatch(/4,?352\s?MB/);
    expect(slider.tabIndex).not.toBe(-1);
  });

  it("number_input_only_while_system_memory_is_unavailable", async () => {
    mount([instance({ memoryMode: "manual", memoryMb: 4096 })], { totalMb: null });
    expect(await screen.findByRole("textbox", { name: "Maximum memory (MB)" })).toBeTruthy();
    expect(screen.queryByRole("slider")).toBeNull();
  });
});
