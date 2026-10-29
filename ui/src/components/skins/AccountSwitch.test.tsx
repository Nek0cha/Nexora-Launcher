import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getSkin, listAccounts } from "../../api/commands";
import { onAccountsChanged } from "../../api/events";
import type { Account } from "../../bindings/Account";
import type { AccountList } from "../../bindings/AccountList";
import i18n from "../../i18n";
import { AccountSwitch } from "./AccountSwitch";

vi.mock("../../api/commands", () => ({ listAccounts: vi.fn(), getSkin: vi.fn() }));
vi.mock("../../api/events", () => ({ onAccountsChanged: vi.fn() }));

const account = (id: string, username: string, patch: Partial<Account> = {}): Account => ({
  id, username, isActive: false, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false, ...patch,
});

function mount(list: AccountList, props: { accountId: string | null; disabled?: boolean; onChange?: (id: string) => void }) {
  vi.mocked(listAccounts).mockResolvedValue(list);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AccountSwitch accountId={props.accountId} onChange={props.onChange ?? (() => {})} disabled={props.disabled ?? false} />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(onAccountsChanged).mockResolvedValue(() => {});
  vi.mocked(getSkin).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("AccountSwitch", () => {
  it("shows only the face and name, not interactive, with a single account", async () => {
    mount({ accounts: [account("a1", "Alex", { isActive: true })], devOffline: null }, { accountId: "a1" });
    expect(await screen.findByText("Alex")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByTestId("skin-face-default")).toBeTruthy();
  });

  it("lists every account with a face and picks one by clicking it", async () => {
    const onChange = vi.fn();
    mount(
      { accounts: [account("a1", "Alex", { isActive: true }), account("a2", "Bob")], devOffline: null },
      { accountId: "a1", onChange },
    );
    const trigger = await screen.findByRole("button", { name: /Accounts/ });
    expect(within(trigger).getByText("Alex")).toBeTruthy();
    fireEvent.click(trigger);
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["Alex", "Bob"]);
    // 項目にも顔が付く
    expect(within(options[1] as HTMLElement).getByTestId("skin-face-default")).toBeTruthy();
    fireEvent.click(options[1] as HTMLElement);
    expect(onChange).toHaveBeenCalledWith("a2");
  });

  it("does not offer the dev offline account", async () => {
    mount(
      { accounts: [account("a1", "Alex", { isActive: true }), account("a2", "Bob")], devOffline: { username: "Player", active: false } },
      { accountId: "a1" },
    );
    fireEvent.click(await screen.findByRole("button", { name: /Accounts/ }));
    expect((await screen.findAllByRole("option")).map((o) => o.textContent)).toEqual(["Alex", "Bob"]);
  });

  it("renders nothing when there is no Microsoft account", async () => {
    const { container } = mount({ accounts: [], devOffline: { username: "Player", active: true } }, { accountId: null });
    await vi.waitFor(() => expect(listAccounts).toHaveBeenCalled());
    expect(container.textContent).toBe("");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("stays pickable when one account exists but none is selected", async () => {
    const onChange = vi.fn();
    mount({ accounts: [account("a1", "Alex")], devOffline: { username: "Player", active: true } }, { accountId: null, onChange });
    fireEvent.click(await screen.findByRole("button", { name: /Accounts/ }));
    fireEvent.click((await screen.findAllByRole("option"))[0] as HTMLElement);
    expect(onChange).toHaveBeenCalledWith("a1");
  });

  it("is disabled while busy and enabled again afterwards", async () => {
    const list = { accounts: [account("a1", "Alex", { isActive: true }), account("a2", "Bob")], devOffline: null };
    const view = mount(list, { accountId: "a1", disabled: true });
    expect(((await screen.findByRole("button", { name: /Accounts/ })) as HTMLButtonElement).disabled).toBe(true);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    view.rerender(
      <QueryClientProvider client={client}>
        <AccountSwitch accountId="a1" onChange={() => {}} disabled={false} />
      </QueryClientProvider>,
    );
    expect(((await screen.findByRole("button", { name: /Accounts/ })) as HTMLButtonElement).disabled).toBe(false);
  });
});
