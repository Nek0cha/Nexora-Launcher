import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createHashRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances } from "../../api/commands";
import type { Instance } from "../../bindings/Instance";
import i18n from "../../i18n";
import { Breadcrumb } from "./Breadcrumb";

vi.mock("../../api/commands", () => ({ listInstances: vi.fn() }));

const instance = (name: string): Instance => ({
  id: "i1", name, dirName: "x", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
});

function mount(path: string, name = "Survival") {
  vi.mocked(listInstances).mockResolvedValue([instance(name)]);
  window.history.replaceState(null, "", `/#${path}`);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createHashRouter([{ path: "*", element: <Breadcrumb /> }]);
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
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

describe("Breadcrumb", () => {
  it("links_every_crumb_but_the_last_and_marks_the_last_as_current", async () => {
    mount("/instances/i1/modrinth");
    const nav = screen.getByTestId("page-name");
    expect(nav.tagName).toBe("NAV");
    const instances = await within(nav).findByRole("link", { name: "Instances" });
    expect(instances.getAttribute("href")).toBe("#/instances");
    await waitFor(() => expect(within(nav).getByRole("link", { name: "Survival" })).toBeTruthy());
    expect(within(nav).getAllByRole("link")).toHaveLength(2);
    const current = within(nav).getByText("Modrinth");
    expect(current.getAttribute("aria-current")).toBe("page");
    expect(current.closest("a")).toBeNull();
  });

  it("hides_the_separators_from_assistive_tech", async () => {
    mount("/instances/i1");
    const nav = screen.getByTestId("page-name");
    await within(nav).findByText("Survival");
    const separators = nav.querySelectorAll("li > svg");
    expect(separators.length).toBe(1);
    for (const s of separators) expect(s.getAttribute("aria-hidden")).toBe("true");
  });

  it("is_not_empty_on_the_shared_settings_screen", () => {
    mount("/sync");
    expect(screen.getByTestId("page-name").textContent).toBe("Shared settings");
  });

  it("keeps_the_parent_crumb_and_truncates_only_the_last_one", async () => {
    mount("/instances/i1", "A very long instance name ".repeat(10));
    const nav = screen.getByTestId("page-name");
    const parent = await within(nav).findByRole("link", { name: "Instances" });
    expect(parent.closest("li")?.className).toContain("shrink-0");
    const last = nav.querySelector("li:last-child");
    expect(last?.className).toContain("min-w-0");
    expect(last?.querySelector("[aria-current]")?.className).toContain("truncate");
  });

  it("truncates_the_middle_crumb_so_the_first_and_last_stay_whole", async () => {
    mount("/instances/i1/modrinth", "A very long instance name ".repeat(10));
    const nav = screen.getByTestId("page-name");
    const middle = await within(nav).findByRole("link", { name: /A very long/ });
    // the middle crumb is the only one allowed to shrink, and it truncates
    expect(middle.closest("li")?.className).toContain("min-w-0");
    expect(middle.closest("li")?.className).not.toContain("shrink-0");
    expect(middle.className).toContain("truncate");
    const items = [...nav.querySelectorAll("li")];
    expect(items).toHaveLength(3);
    expect(items[0]?.className).toContain("shrink-0");
    expect(items[2]?.className).toContain("shrink-0");
    expect(items[2]?.className).not.toContain("min-w-0");
  });
});
