import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createInstance, listInstances, listVersions } from "../api/commands";
import type { Instance } from "../bindings/Instance";
import { CommandPalette } from "../components/CommandPalette";
import { pickHomeInstance } from "../features/instances/useInstances";
import i18n from "../i18n";
import Home from "./Home";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn().mockResolvedValue([]),
  listVersions: vi.fn(),
  createInstance: vi.fn(),
  launchInstance: vi.fn(),
  cancelTask: vi.fn(),
  setSetting: vi.fn(),
}));

const played: Instance = {
  id: "old", name: "Old World", dirName: "Old World", mcVersion: "1.20.1", state: "ready",
  createdAt: 1, lastPlayedAt: 100, playTimeMs: 1000,
};
const fresh: Instance = {
  id: "new", name: "New World", dirName: "New World", mcVersion: "1.21.5", state: "installing",
  createdAt: 200, lastPlayedAt: null, playTimeMs: 0,
};

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("home selection", () => {
  it("falls back to the first instance when nothing or a missing instance is selected", () => {
    expect(pickHomeInstance([played, fresh], null)).toBe(played);
    expect(pickHomeInstance([played, fresh], "gone")).toBe(played);
    expect(pickHomeInstance([played, fresh], "new")).toBe(fresh);
    expect(pickHomeInstance([], "new")).toBeUndefined();
  });

  it("shows a newly created instance on home and in the palette even though it sorts after a played one", async () => {
    // backend は遊んだことのあるインスタンスを先に並べる
    vi.mocked(listInstances).mockResolvedValue([played]);
    vi.mocked(listVersions).mockResolvedValue([{ id: "1.21.5", releaseTime: "2025-03-25T00:00:00Z" }]);
    vi.mocked(createInstance).mockImplementation(async () => {
      vi.mocked(listInstances).mockResolvedValue([played, fresh]);
      return { instance: fresh, taskId: "t1" };
    });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <Home />
          <CommandPalette />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(await screen.findByRole("heading", { name: "Old World" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Create instance" }));
    const dialog = await screen.findByRole("dialog");
    const name = within(dialog).getByRole("textbox", { name: "Name" }) as HTMLInputElement;
    await waitFor(() => expect(name.value).toBe("1.21.5"));
    fireEvent.change(name, { target: { value: "New World" } });
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Create" })));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    expect(await screen.findByRole("heading", { name: "New World" })).toBeTruthy();
    await act(async () => {
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    });
    expect(await screen.findByRole("menuitem", { name: "Launch New World" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Launch Old World" })).toBeNull();
  });
});
