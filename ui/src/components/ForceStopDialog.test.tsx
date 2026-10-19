import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { stopInstance } from "../api/commands";
import { clearStop, STOP_KEY } from "../features/play/useStopFlow";
import i18n from "../i18n";
import { ForceStopDialog } from "./ForceStopDialog";

vi.mock("../api/commands", () => ({ stopInstance: vi.fn() }));

function mount() {
  const client = new QueryClient();
  client.setQueryData(STOP_KEY, { states: { i1: "waiting" }, confirm: ["i1"] });
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
