import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { InstanceSync } from "../../bindings/InstanceSync";
import { useInstanceSync } from "./useInstanceSync";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

const notJoined: InstanceSync = {
  profile: null,
  members: [],
  lastApplied: null,
  pendingCount: 0,
  profilePendingCount: 0,
  excluded: [],
  hasOptions: true,
  applicability: "ready",
  upToDate: false,
  overrides: [],
};

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(() => useInstanceSync("i1"), { wrapper });
}

describe("useInstanceSync", () => {
  it("reads the sync state of the instance", async () => {
    vi.mocked(invoke).mockResolvedValue(notJoined);
    const hook = setup();
    await waitFor(() => expect(hook.result.current.data).toEqual(notJoined));
    expect(invoke).toHaveBeenCalledWith("get_instance_sync", { instanceId: "i1" });
  });

  it("refresh reads the state again and is busy until the answer arrives", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(notJoined);
    const hook = setup();
    await waitFor(() => expect(hook.result.current.data).toEqual(notJoined));
    expect(hook.result.current.busy).toBe(false);

    let answer!: (value: InstanceSync) => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise((resolve) => (answer = resolve)));
    act(() => hook.result.current.refresh());
    await waitFor(() => expect(hook.result.current.busy).toBe(true));
    expect(invoke).toHaveBeenCalledTimes(2);

    const joined = { ...notJoined, upToDate: true };
    await act(async () => answer(joined));
    await waitFor(() => expect(hook.result.current.data).toEqual(joined));
    expect(hook.result.current.busy).toBe(false);
  });
});
