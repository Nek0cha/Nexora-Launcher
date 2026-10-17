import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listAccounts } from "../../api/commands";
import { onAccountsChanged } from "../../api/events";
import type { Account } from "../../bindings/Account";
import type { AccountList } from "../../bindings/AccountList";
import { useAccounts } from "./useAccounts";

vi.mock("../../api/commands", () => ({ listAccounts: vi.fn() }));
vi.mock("../../api/events", () => ({ onAccountsChanged: vi.fn() }));

const account: Account = {
  id: "a", username: "u", isActive: true, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false,
};
let changed: () => void;
const unlisten = vi.fn();

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return renderHook(() => useAccounts(), {
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  });
}

beforeEach(() => {
  vi.mocked(onAccountsChanged).mockImplementation(async (h) => { changed = h; return unlisten; });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

describe("useAccounts", () => {
  it("refetches on accounts://changed", async () => {
    vi.mocked(listAccounts).mockResolvedValueOnce({ accounts: [], devOffline: null });
    const { result } = mount();
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.active).toBeNull();
    vi.mocked(listAccounts).mockResolvedValue({ accounts: [account], devOffline: null });
    await waitFor(() => expect(changed).toBeDefined());
    changed();
    await waitFor(() => expect(result.current.active).toEqual({ kind: "msa", account }));
    expect(listAccounts).toHaveBeenCalledTimes(2);
  });

  it("returns the dev offline account when it is active", async () => {
    const list: AccountList = {
      accounts: [{ ...account, isActive: false }],
      devOffline: { username: "Dev", active: true },
    };
    vi.mocked(listAccounts).mockResolvedValue(list);
    const { result } = mount();
    await waitFor(() => expect(result.current.active).toEqual({ kind: "devOffline", username: "Dev" }));
  });

  it("stops listening on unmount", async () => {
    vi.mocked(listAccounts).mockResolvedValue({ accounts: [], devOffline: null });
    const { unmount } = mount();
    await waitFor(() => expect(onAccountsChanged).toHaveBeenCalled());
    await Promise.resolve();
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
