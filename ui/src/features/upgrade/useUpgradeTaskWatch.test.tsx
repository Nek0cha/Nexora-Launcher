import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import i18n from "../../i18n";
import { INSTANCES_KEY } from "../instances/useInstances";
import type { TaskTable } from "../tasks/taskTable";
import { useUpgradeTaskWatch } from "./useUpgradeTaskWatch";

let table: TaskTable = {};
const showError = vi.fn();
vi.mock("../tasks/useTasks", () => ({ useTasks: () => table }));
vi.mock("../../components/toast/useToast", () => ({ useToast: () => ({ show: vi.fn(), showError }) }));

const error = { code: "upgrade.unavailable", params: { reason: "x" }, detail: null };
const task = (state: TaskSnapshot["state"], extra: Partial<TaskSnapshot> = {}): TaskSnapshot =>
  ({ id: "t1", kind: "instance-upgrade", instanceId: "i1", state, error: null, ...extra }) as TaskSnapshot;

function mount() {
  const client = new QueryClient();
  const spy = vi.spyOn(client, "invalidateQueries");
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const view = renderHook(() => useUpgradeTaskWatch(), { wrapper });
  return { spy, rerender: () => view.rerender() };
}

afterEach(() => {
  table = {};
  vi.clearAllMocks();
});

describe("useUpgradeTaskWatch", () => {
  it("refetches the instances when an upgrade finishes and tells the user why a failed one failed", () => {
    table = { t1: task("running") };
    const { spy, rerender } = mount();
    expect(spy).not.toHaveBeenCalled();

    table = { t1: task("failed", { error }) };
    rerender();

    expect(spy).toHaveBeenCalledWith({ queryKey: INSTANCES_KEY });
    expect(showError).toHaveBeenCalledWith(error);
  });

  it("refetches without a toast when the upgrade succeeds", () => {
    table = { t1: task("running") };
    const { spy, rerender } = mount();
    table = { t1: task("succeeded") };
    rerender();
    expect(spy).toHaveBeenCalledWith({ queryKey: INSTANCES_KEY });
    expect(showError).not.toHaveBeenCalled();
  });

  it("ignores a finished task it never saw running, and other kinds", () => {
    table = { t1: task("failed", { error }), t2: task("failed", { id: "t2", kind: "install", error }) };
    const { spy } = mount();
    expect(spy).not.toHaveBeenCalled();
    expect(showError).not.toHaveBeenCalled();
  });

  it("has a readable task label in both languages", async () => {
    await i18n.changeLanguage("en");
    expect(i18n.t("tasks.kind.instance-upgrade")).toBe("Upgrade version");
    await i18n.changeLanguage("ja");
    expect(i18n.t("tasks.kind.instance-upgrade")).toBe("バージョンを上げる");
  });
});
