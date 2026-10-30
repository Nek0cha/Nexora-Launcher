import { describe, expect, it } from "vitest";
import type { Account } from "../../bindings/Account";
import { canApply } from "./canApply";

const account = (needsLogin: boolean) => ({ id: "a1", needsLogin }) as unknown as Account;

describe("canApply", () => {
  it("is true for a signed-in Microsoft account", () => {
    expect(canApply({ kind: "msa", account: account(false) })).toBe(true);
  });
  it("is false when the account needs to log in again", () => {
    expect(canApply({ kind: "msa", account: account(true) })).toBe(false);
  });
  it("is false for the dev offline account and for no account", () => {
    expect(canApply({ kind: "devOffline", username: "Dev" })).toBe(false);
    expect(canApply(null)).toBe(false);
  });
});
