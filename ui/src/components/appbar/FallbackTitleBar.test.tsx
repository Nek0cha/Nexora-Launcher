import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { createHashRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppBar } from "./AppBar";
import { FallbackTitleBar } from "./FallbackTitleBar";

vi.mock("../../api/commands", () => ({ listInstances: vi.fn().mockResolvedValue([]), cancelTask: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(),
    isMaximized: vi.fn().mockResolvedValue(false), onResized: vi.fn().mockResolvedValue(() => {}),
  }),
}));
const platform = { windows: true, mac: false };
vi.mock("../../app/platform", () => ({ useIsWindows: () => platform.windows, useIsMac: () => platform.mac }));

afterEach(() => {
  cleanup();
  platform.windows = true;
  platform.mac = false;
});

describe("FallbackTitleBar", () => {
  it("lays_out_the_brand_exactly_like_the_app_bar", () => {
    const client = new QueryClient();
    render(<FallbackTitleBar />);
    const fallback = screen.getByTestId("fallback-title-bar");
    const fallbackBrand = screen.getByTestId("brand").outerHTML;
    const fallbackPad = fallback.className.match(/\bpl-\d+\b/)?.[0];
    cleanup();

    const router = createHashRouter([{ path: "*", element: <AppBar showWindowControls /> }]);
    render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    const bar = screen.getByTestId("app-bar");
    expect(screen.getByTestId("brand").outerHTML).toBe(fallbackBrand);
    expect(bar.className.match(/\bpl-\d+\b/)?.[0]).toBe(fallbackPad);
  });

  it("keeps_clear_of_the_traffic_lights_on_macos_and_has_no_window_buttons", () => {
    platform.windows = false;
    platform.mac = true;
    render(<FallbackTitleBar />);
    expect(screen.getByTestId("fallback-title-bar").className).toContain("pl-[78px]");
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
  });

  it("renders_nothing_on_other_systems", () => {
    platform.windows = false;
    render(<FallbackTitleBar />);
    expect(screen.queryByTestId("fallback-title-bar")).toBeNull();
  });
});
