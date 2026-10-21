import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { getSettings, listContentCategories, listInstances, searchModrinth } from "../api/commands";
import type { Instance } from "../bindings/Instance";
import i18n from "../i18n";
import InstanceModrinth from "./InstanceModrinth";

vi.mock("../api/commands", () => ({
  searchModrinth: vi.fn(),
  listInstances: vi.fn(),
  getSettings: vi.fn(),
  listContentCategories: vi.fn(),
  listVersions: vi.fn(),
  previewContentInstall: vi.fn(),
  installContent: vi.fn(),
  getTask: vi.fn(),
  contentTaskReport: vi.fn(),
  openModrinthProject: vi.fn(),
}));

const instance = (patch: Partial<Instance> = {}): Instance => ({
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.20.1", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "neoforge", loaderVersion: "20.4.1",
  ...patch,
});

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  vi.mocked(searchModrinth).mockResolvedValue({ hits: [], offset: 0, total: 0 });
  vi.mocked(getSettings).mockResolvedValue({
    theme: "system", language: null, maxConcurrentDownloads: 8, lastAddedInstance: null,
  });
  vi.mocked(listContentCategories).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

function DetailStub() {
  const location = useLocation();
  return <p>detail page {JSON.stringify(location.state)}</p>;
}

function mount(list: Instance[], id = "i1") {
  vi.mocked(listInstances).mockResolvedValue(list);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[`/instances/${id}/modrinth`]}>
        <Routes>
          <Route path="/instances/:id/modrinth" element={<InstanceModrinth />} />
          <Route path="/instances/:id" element={<DetailStub />} />
          <Route path="/instances" element={<p>list page</p>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("InstanceModrinth", () => {
  it("searches with the instance's version and loader fixed", async () => {
    mount([instance()]);
    expect(await screen.findByRole("heading", { level: 1, name: "Add mods to Survival" })).toBeTruthy();
    expect(await screen.findByText("Fixed: 1.20.1 / NeoForge")).toBeTruthy();
    expect(screen.queryByRole("toolbar", { name: "Loader" })).toBeNull();
    expect(searchModrinth).toHaveBeenCalledWith(
      "i1",
      expect.objectContaining({ mcVersion: "1.20.1", loaders: ["neoforge"] }),
      0,
    );
  });

  it("goes back to the content tab of the instance", async () => {
    mount([instance()]);
    fireEvent.click(await screen.findByRole("link", { name: "Survival" }));
    expect(await screen.findByText('detail page {"tab":"content"}')).toBeTruthy();
  });

  it("explains that a vanilla instance can't take mods", async () => {
    mount([instance({ loader: "vanilla", loaderVersion: null })]);
    expect(await screen.findByText("This instance has no mod loader, so mods can't be added.")).toBeTruthy();
    expect(searchModrinth).not.toHaveBeenCalled();
  });

  it("shows not found for an unknown instance", async () => {
    mount([instance()], "nope");
    expect(await screen.findByText("This instance doesn't exist.")).toBeTruthy();
  });
});
