import { describe, expect, it } from "vitest";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { isAllowedVerificationUri, loginPrompt, sameAccount } from "./loginState";

const err = (code: string, params: ErrorPayload["params"] = {}): ErrorPayload => ({ code, params, detail: null });

describe("loginPrompt", () => {
  it("loginPrompt maps auth errors", () => {
    expect(loginPrompt(err("auth.login_required", { accountId: "abc" }))).toEqual({ kind: "loginRequired", accountId: "abc" });
    expect(loginPrompt(err("auth.no_account"))).toEqual({ kind: "noAccount" });
    expect(loginPrompt(err("auth.offline", { offlineAvailable: true }))).toEqual({ kind: "offline", offlineAvailable: true });
    expect(loginPrompt(err("auth.offline", { offlineAvailable: false }))).toEqual({ kind: "offline", offlineAvailable: false });
    // 引数が欠けていれば、オフラインでは遊べないものとして扱う
    expect(loginPrompt(err("auth.offline"))).toEqual({ kind: "offline", offlineAvailable: false });
    expect(loginPrompt(err("auth.login_required"))).toEqual({ kind: "loginRequired", accountId: "" });
    expect(loginPrompt(err("auth.timeout"))).toBeNull();
    expect(loginPrompt(err("launch.spawn_failed"))).toBeNull();
    expect(loginPrompt(null)).toBeNull();
  });
});

describe("isAllowedVerificationUri", () => {
  it("allows only the two fixed Microsoft pages", () => {
    expect(isAllowedVerificationUri("https://www.microsoft.com/link")).toBe(true);
    expect(isAllowedVerificationUri("https://microsoft.com/devicelogin")).toBe(true);
    expect(isAllowedVerificationUri("https://www.microsoft.com/link?x=1")).toBe(false);
    expect(isAllowedVerificationUri("http://www.microsoft.com/link")).toBe(false);
    expect(isAllowedVerificationUri("https://evil.example/link")).toBe(false);
  });
});

describe("sameAccount", () => {
  it("ignores hyphens and case", () => {
    expect(sameAccount("0123456789ABCDEF0123456789abcdef", "01234567-89ab-cdef-0123-456789abcdef")).toBe(true);
    expect(sameAccount("a", "b")).toBe(false);
    expect(sameAccount("", "")).toBe(false);
  });
});
