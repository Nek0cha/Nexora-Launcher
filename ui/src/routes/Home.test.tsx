import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import i18n from "../i18n";
import Home from "./Home";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn().mockResolvedValue([]),
  listVersions: vi.fn(),
  createInstance: vi.fn(),
  launchInstance: vi.fn(),
  cancelTask: vi.fn(),
}));

const world: Instance = {
  id: "i1", name: "World", dirName: "World", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
};

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Home />
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

describe("Home", () => {
  it("offers to create an instance when there is none", async () => {
    vi.mocked(listInstances).mockResolvedValue([]);
    mount();
    expect(await screen.findByText("No instances yet. Pick a version to create one.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create instance" })).toBeTruthy();
  });

  it("shows the instance with its version, the play button and a quiet create action", async () => {
    vi.mocked(listInstances).mockResolvedValue([world]);
    mount();
    expect(await screen.findByRole("heading", { name: "World" })).toBeTruthy();
    expect(screen.getByText("1.21.5")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create instance" })).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Play" })).toBeTruthy();
  });

  it("shows the error when the list cannot be loaded", async () => {
    vi.mocked(listInstances).mockRejectedValue(new IpcError({ code: "db.failed", params: {}, detail: null }));
    mount();
    expect(await screen.findByText("A database operation failed")).toBeTruthy();
    expect(screen.queryByText("No instances yet. Pick a version to create one.")).toBeNull();
  });
});
