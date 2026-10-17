import { describe, expect, it } from "vitest";
import type { Account } from "../../bindings/Account";
import { accountStatus } from "./accountState";

const base: Account = {
  id: "a", username: "u", isActive: false, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false,
};

describe("accountStatus", () => {
  it("prefers needsLogin over everything", () => {
    expect(accountStatus({ ...base, needsLogin: true, isActive: true, loginSaved: false, entitlement: "unknown" })).toBe("needsLogin");
  });
  it("marks unsaved logins", () => {
    expect(accountStatus({ ...base, loginSaved: false, isActive: true, entitlement: "unknown" })).toBe("unsaved");
  });
  it("marks unknown ownership before active", () => {
    expect(accountStatus({ ...base, entitlement: "unknown", isActive: true })).toBe("ownershipUnknown");
  });
  it("marks the active and ordinary accounts", () => {
    expect(accountStatus({ ...base, isActive: true })).toBe("active");
    expect(accountStatus(base)).toBe("ok");
  });
});
