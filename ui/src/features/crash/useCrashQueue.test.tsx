import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { CRASH_KEY, pushCrash, useCrashQueue } from "./useCrashQueue";

afterEach(cleanup);

function setup() {
  const client = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, view: renderHook(() => useCrashQueue(), { wrapper }) };
}

describe("useCrashQueue", () => {
  it("shows the first crash and moves on to the next when dismissed", async () => {
    const { client, view } = setup();
    expect(view.result.current.head).toBeNull();
    await act(async () => pushCrash(client, { instanceId: "i1", launchedAt: 1 }));
    await act(async () => pushCrash(client, { instanceId: "i2", launchedAt: 2 }));
    await waitFor(() => expect(view.result.current.head).toEqual({ instanceId: "i1", launchedAt: 1 }));
    await act(async () => view.result.current.dismiss());
    await waitFor(() => expect(view.result.current.head).toEqual({ instanceId: "i2", launchedAt: 2 }));
    await act(async () => view.result.current.dismiss());
    await waitFor(() => expect(view.result.current.head).toBeNull());
    expect(client.getQueryData(CRASH_KEY)).toEqual([]);
  });

  it("does not queue the same run twice", async () => {
    const { client, view } = setup();
    await act(async () => pushCrash(client, { instanceId: "i1", launchedAt: 1 }));
    await act(async () => pushCrash(client, { instanceId: "i1", launchedAt: 1 }));
    expect(client.getQueryData(CRASH_KEY)).toEqual([{ instanceId: "i1", launchedAt: 1 }]);
    await waitFor(() => expect(view.result.current.head).not.toBeNull());
  });
});
