import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances, listRunning, updateInstance } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import { RUNNING_KEY } from "../features/play/useGameSync";
import i18n from "../i18n";
import InstanceDetail from "./InstanceDetail";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  updateInstance: vi.fn(),
}));

const instance = (patch: Partial<Instance> = {}): Instance => ({
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  ...patch,
});

function mount(list: Instance[], { id = "i1", running = [] as RunningSession[] } = {}) {
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
    </QueryClientProvider>,
  );
}

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

  it("save_failure_shows_failure_notice", async () => {
    vi.mocked(updateInstance).mockRejectedValue(
      new IpcError({ code: "instance.invalid_setting", params: {}, detail: null }),
    );
    mount([instance()]);
    await saveButton();
    fireEvent.change(nameInput(), { target: { value: "Creative" } });
    await press(await saveButton());
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText("A setting value is not valid")).toBeTruthy();
  });
});
