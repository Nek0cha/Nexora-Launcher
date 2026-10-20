import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances, stopInstance } from "../api/commands";
import type { Instance } from "../bindings/Instance";
import { INSTANCES_KEY } from "../features/instances/useInstances";
import { clearStop, STOP_KEY } from "../features/play/useStopFlow";
import i18n from "../i18n";
import { ForceStopDialog } from "./ForceStopDialog";

vi.mock("../api/commands", () => ({ stopInstance: vi.fn(), listInstances: vi.fn() }));

const instance = (id: string, name: string): Instance => ({
  id, name, dirName: name, mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
});
const instances = [instance("i1", "Alpha"), instance("i2", "Beta")];

function mount(confirm: string[] = ["i1"]) {
  const client = new QueryClient();
  vi.mocked(listInstances).mockResolvedValue(instances);
  client.setQueryData(INSTANCES_KEY, instances);
  client.setQueryData(STOP_KEY, {
    states: Object.fromEntries(confirm.map((id) => [id, "waiting"])),
    confirm,
  });
  render(
    <QueryClientProvider client={client}>
      <ForceStopDialog />
    </QueryClientProvider>,
  );
  const table = () => client.getQueryData<{ states: Record<string, string>; confirm: string[] }>(STOP_KEY);
  return { client, table };
}

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  vi.mocked(stopInstance).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ForceStopDialog", () => {
  it("uses the spec wording and focuses Wait first", async () => {
    mount();
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("ゲームが閉じません");
    expect(dialog.textContent).toContain("強制終了すると、最後のセーブ以降の進みが失われるおそれがあります。");
    expect(dialog.textContent).toContain("対象：Alpha");
    await waitFor(() => expect(document.activeElement?.textContent).toBe("待つ"));
  });

  it("wait closes the dialog and keeps waiting with force ready", async () => {
    const { table } = mount();
    await act(async () => {
      fireEvent.click(await screen.findByRole("button", { name: "待つ" }));
    });
    expect(table()).toEqual({ states: { i1: "forceReady" }, confirm: [] });
    expect(stopInstance).not.toHaveBeenCalled();
  });

  it("force stop calls with force true", async () => {
    mount();
    await act(async () => {
      fireEvent.click(await screen.findByRole("button", { name: "強制終了" }));
    });
    expect(stopInstance).toHaveBeenCalledWith("i1", true);
  });

  it("asks for a second queued instance with focus back on Wait", async () => {
    const { table } = mount(["i1", "i2"]);
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("対象：Alpha");
    const force = screen.getByRole("button", { name: "強制終了" });
    await act(async () => {
      force.focus();
      fireEvent.click(force);
    });
    expect(stopInstance).toHaveBeenCalledTimes(1);
    expect(stopInstance).toHaveBeenCalledWith("i1", true);
    await waitFor(() => expect(screen.getByRole("alertdialog").textContent).toContain("対象：Beta"));
    // 続けて Enter を押しても、2 つ目を強制終了しないよう、フォーカスは「待つ」に戻る
    await waitFor(() => expect(document.activeElement?.textContent).toBe("待つ"));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "待つ" }));
    });
    expect(table()).toEqual({ states: { i1: "forceReady", i2: "forceReady" }, confirm: [] });
    expect(stopInstance).toHaveBeenCalledTimes(1);
  });

  it("closes without forcing when the game exits", async () => {
    const { client } = mount();
    await screen.findByRole("alertdialog");
    await act(async () => {
      clearStop(client, "i1");
    });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(stopInstance).not.toHaveBeenCalled();
  });
});
