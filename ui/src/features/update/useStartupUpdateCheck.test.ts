import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppInfo } from "../../bindings/AppInfo";
import { checkForUpdate } from "./updateStore";
import { useStartupUpdateCheck } from "./useStartupUpdateCheck";

vi.mock("./updateStore", () => ({ checkForUpdate: vi.fn() }));
afterEach(() => vi.resetAllMocks());

const info = (over: Partial<AppInfo>): AppInfo => ({
  version: "0.1.0",
  build: "release",
  os: "windows",
  dataDir: "",
  materialAvailable: false,
  selfUpdate: true,
  ...over,
});

describe("useStartupUpdateCheck", () => {
  it("checks once silently in a release build that can self update", () => {
    const { rerender } = renderHook(({ i }) => useStartupUpdateCheck(i), { initialProps: { i: info({}) } });
    rerender({ i: info({ version: "0.1.0" }) });
    expect(checkForUpdate).toHaveBeenCalledTimes(1);
    expect(checkForUpdate).toHaveBeenCalledWith({ manual: false });
  });

  it("does not check in a dev build", () => {
    renderHook(() => useStartupUpdateCheck(info({ build: "dev" })));
    expect(checkForUpdate).not.toHaveBeenCalled();
  });

  it("does not check when self update is unsupported", () => {
    renderHook(() => useStartupUpdateCheck(info({ selfUpdate: false })));
    expect(checkForUpdate).not.toHaveBeenCalled();
  });

  it("waits until the app info is loaded", () => {
    const { rerender } = renderHook(({ i }) => useStartupUpdateCheck(i), {
      initialProps: { i: undefined as AppInfo | undefined },
    });
    expect(checkForUpdate).not.toHaveBeenCalled();
    rerender({ i: info({}) });
    expect(checkForUpdate).toHaveBeenCalledTimes(1);
  });
});
