import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { diagnoseCrash, listInstances, setContentEnabled } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { CrashCandidate } from "../bindings/CrashCandidate";
import type { CrashDiagnosis } from "../bindings/CrashDiagnosis";
import type { Instance } from "../bindings/Instance";
import { pushCrash } from "../features/crash/useCrashQueue";
import { INSTANCES_KEY } from "../features/instances/useInstances";
import i18n from "../i18n";
import { CrashDialog } from "./CrashDialog";

vi.mock("../api/commands", () => ({ diagnoseCrash: vi.fn(), setContentEnabled: vi.fn(), listInstances: vi.fn() }));

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "fabric", loaderVersion: "0.16.0",
};

const candidate = (relPath: string, patch: Partial<CrashCandidate> = {}): CrashCandidate => ({
  relPath, name: relPath.replace("mods/", ""), modId: relPath, reasons: ["missingDependency"],
  confidence: "high", evidence: "line", missing: "bar", ...patch,
});

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + JSON.stringify(location.state)}</p>;
}

function mount(diagnosis: CrashDiagnosis | Error) {
  if (diagnosis instanceof Error) vi.mocked(diagnoseCrash).mockRejectedValue(diagnosis);
  else vi.mocked(diagnoseCrash).mockResolvedValue(diagnosis);
  vi.mocked(listInstances).mockResolvedValue([instance]);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(INSTANCES_KEY, [instance]);
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route
            path="*"
            element={
              <>
                <CrashDialog />
                <Where />
              </>
            }
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  act(() => pushCrash(client, { instanceId: "i1", launchedAt: 42 }));
  return client;
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("CrashDialog", () => {
  it("lists the candidates, checking only the high confidence ones", async () => {
    mount({
      logName: "42.log",
      candidates: [
        candidate("mods/foo.jar"),
        candidate("mods/stack.jar", { confidence: "low", reasons: ["stackTrace"], missing: null }),
      ],
    });
    const foo = await screen.findByRole("checkbox", { name: /foo\.jar/ });
    expect((foo as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("checkbox", { name: /stack\.jar/ }) as HTMLInputElement).checked).toBe(false);
    expect(screen.getByText(/bar/)).toBeTruthy();
    expect(diagnoseCrash).toHaveBeenCalledWith("i1", 42);
  });

  it("disables only the checked mods and closes when all succeed", async () => {
    vi.mocked(setContentEnabled).mockResolvedValue(null);
    mount({ logName: "42.log", candidates: [candidate("mods/a.jar"), candidate("mods/b.jar")] });
    fireEvent.click(await screen.findByRole("checkbox", { name: /b\.jar/ }));
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));
    await waitFor(() => expect(setContentEnabled).toHaveBeenCalledTimes(1));
    expect(setContentEnabled).toHaveBeenCalledWith("i1", "mod", "mods/a.jar", false);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("keeps a failed mod in the list while the rest are disabled", async () => {
    vi.mocked(setContentEnabled).mockImplementation(async (_i, _k, relPath) => {
      if (relPath === "mods/a.jar") throw new IpcError({ code: "content.exists", params: {}, detail: null });
      return null;
    });
    mount({ logName: "42.log", candidates: [candidate("mods/a.jar"), candidate("mods/b.jar")] });
    await screen.findByRole("checkbox", { name: /a\.jar/ });
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));
    await waitFor(() => expect(setContentEnabled).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: /b\.jar/ })).toBeNull());
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByRole("checkbox", { name: /a\.jar/ })).toBeTruthy();
    expect(within(dialog).getByRole("alert")).toBeTruthy();
  });

  it("offers only the log when there are no candidates, and opens the logs tab", async () => {
    mount({ logName: "42.log", candidates: [] });
    await screen.findByRole("alertdialog");
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByRole("button", { name: "Disable" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open log" }));
    expect(screen.getByTestId("where").textContent).toBe('/instances/i1{"tab":"logs"}');
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("shows the same no-candidates dialog when the diagnosis fails", async () => {
    mount(new IpcError({ code: "db.failed", params: {}, detail: null }));
    await screen.findByRole("alertdialog");
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByRole("button", { name: "Open log" })).toBeTruthy();
  });

  it("translates every crash reason in both languages", async () => {
    const reasons = [
      "missingDependency",
      "incompatibleVersion",
      "modLoadingError",
      "mixinFailure",
      "suspectedInReport",
      "stackTrace",
    ];
    for (const lng of ["en", "ja"]) {
      await i18n.changeLanguage(lng);
      for (const reason of reasons) expect(i18n.exists(`crash.reason.${reason}`)).toBe(true);
    }
  });
});
