import { cleanup, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import "../i18n";
import { getStartupStatus } from "../api/commands";
import { IpcError } from "../api/ipc";
import { StartupGate } from "./StartupGate";

vi.mock("../api/commands", () => ({ getStartupStatus: vi.fn() }));

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <StartupGate>
        <p>app</p>
      </StartupGate>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.mocked(getStartupStatus).mockReset();
});

describe("StartupGate", () => {
  it("renders children when ready", async () => {
    vi.mocked(getStartupStatus).mockResolvedValue({ state: "ready" });
    mount();
    expect(await screen.findByText("app")).toBeTruthy();
  });

  it("shows the startup error screen when the status reports a failure", async () => {
    vi.mocked(getStartupStatus).mockResolvedValue({
      state: "failed",
      error: { code: "db.migration_failed", params: {}, detail: "bad file" },
    });
    mount();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("db.migration_failed");
    expect(screen.queryByText("app")).toBeNull();
  });

  it("shows the startup error screen when the status cannot be fetched", async () => {
    vi.mocked(getStartupStatus).mockRejectedValue(
      new IpcError({ code: "app.unexpected", params: {}, detail: "ipc unavailable" }),
    );
    mount();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("app.unexpected");
    expect(alert.textContent).toContain("ipc unavailable");
  });
});
