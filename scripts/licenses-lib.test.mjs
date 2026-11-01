import assert from "node:assert/strict";
import { test } from "node:test";
import { isAccepted, renderNotices } from "./licenses-lib.mjs";

test("a single accepted license passes", () => {
  assert.equal(isAccepted("MIT"), true);
});

test("OR passes when any alternative is accepted", () => {
  assert.equal(isAccepted("GPL-3.0-only OR MIT"), true);
  assert.equal(isAccepted("MIT/Apache-2.0"), true);
});

test("AND needs every part accepted", () => {
  assert.equal(isAccepted("MIT AND Unicode-3.0"), true);
  assert.equal(isAccepted("MIT AND GPL-3.0-only"), false);
});

test("WITH exceptions are judged by the base license", () => {
  assert.equal(isAccepted("Apache-2.0 WITH LLVM-exception"), true);
});

test("unknown and empty expressions fail", () => {
  assert.equal(isAccepted("GPL-3.0-only"), false);
  assert.equal(isAccepted(""), false);
});

test("packages with the same text share one block", () => {
  const out = renderNotices([
    { name: "b", version: "1.0.0", license: "MIT", text: "same text" },
    { name: "a", version: "2.0.0", license: "MIT", text: "same text" },
    { name: "c", version: "1.0.0", license: "ISC", text: "other text" },
  ]);
  assert.equal(out.split("same text").length - 1, 1);
  assert.ok(out.indexOf("a 2.0.0 (MIT)") < out.indexOf("b 1.0.0 (MIT)"));
  assert.ok(out.includes("other text"));
});
