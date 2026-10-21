import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { getSettings, listContentCategories, listInstances, listVersions, searchModrinth } from "../api/commands";
import i18n from "../i18n";
import Browse from "./Browse";

vi.mock("../api/commands", () => ({
  identifyContent: vi.fn(),
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

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  vi.mocked(searchModrinth).mockResolvedValue({ hits: [], offset: 0, total: 0 });
  vi.mocked(listInstances).mockResolvedValue([]);
  vi.mocked(getSettings).mockResolvedValue({
    theme: "system", language: null, maxConcurrentDownloads: 8, lastAddedInstance: null,
  });
  vi.mocked(listContentCategories).mockResolvedValue([]);
  vi.mocked(listVersions).mockResolvedValue([{ id: "1.21", releaseTime: "2024-06-13T00:00:00Z" }]);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("Browse", () => {
  it("shows the global search with free version and loader filters", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <Browse />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(screen.getByRole("heading", { level: 1, name: "Browse mods" })).toBeTruthy();
    expect(await screen.findByRole("toolbar", { name: "Loader" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Minecraft version/ })).toBeTruthy();
    expect(screen.queryByText(/^Fixed:/)).toBeNull();
    expect(searchModrinth).toHaveBeenCalledWith(null, expect.objectContaining({ mcVersion: null, loaders: [] }), 0);
  });
});
