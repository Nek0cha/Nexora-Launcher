import { act, cleanup, render, waitFor } from "@testing-library/react";
import { createElement } from "react";
import { createHashRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { type HistoryNav, useHistoryNav } from "./useHistoryNav";

let nav: HistoryNav;

function Probe() {
  nav = useHistoryNav();
  return null;
}

function mount() {
  window.history.replaceState(null, "", "/");
  const router = createHashRouter([{ path: "*", element: createElement(Probe) }]);
  render(createElement(RouterProvider, { router }));
  return router;
}

afterEach(() => {
  cleanup();
});

describe("useHistoryNav", () => {
  it("disables back at the start, enables forward after going back, and disables it again at the end", async () => {
    const router = mount();
    expect(nav.canGoBack).toBe(false);
    expect(nav.canGoForward).toBe(false);

    await act(async () => {
      await router.navigate("/instances");
    });
    await act(async () => {
      await router.navigate("/accounts");
    });
    expect(nav.canGoBack).toBe(true);
    expect(nav.canGoForward).toBe(false);

    act(() => nav.back());
    await waitFor(() => expect(router.state.location.pathname).toBe("/instances"));
    expect(nav.canGoBack).toBe(true);
    expect(nav.canGoForward).toBe(true);

    act(() => nav.forward());
    await waitFor(() => expect(router.state.location.pathname).toBe("/accounts"));
    expect(nav.canGoForward).toBe(false);
  });

  it("drops the forward history after navigating somewhere new", async () => {
    const router = mount();
    await act(async () => {
      await router.navigate("/instances");
    });
    await act(async () => {
      await router.navigate("/accounts");
    });
    act(() => nav.back());
    await waitFor(() => expect(nav.canGoForward).toBe(true));

    await act(async () => {
      await router.navigate("/settings");
    });
    expect(nav.canGoForward).toBe(false);
    expect(nav.canGoBack).toBe(true);
  });

  it("disables back again after returning to the first page", async () => {
    const router = mount();
    await act(async () => {
      await router.navigate("/instances");
    });
    act(() => nav.back());
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
    expect(nav.canGoBack).toBe(false);
    expect(nav.canGoForward).toBe(true);
  });
});
