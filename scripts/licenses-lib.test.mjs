import assert from "node:assert/strict";
import { test } from "node:test";
import { buildNotices, isAccepted } from "./licenses-lib.mjs";

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

test("packages are sorted by name and share one text when the text is the same", () => {
  const { entries, texts } = buildNotices([
    { name: "b", version: "1.0.0", license: "MIT", text: "same text" },
    { name: "a", version: "2.0.0", license: "MIT", text: "same text\n" },
    { name: "c", version: "1.0.0", license: "ISC", text: "other text" },
  ]);
  assert.deepEqual(
    entries.map((e) => e.name),
    ["a", "b", "c"],
  );
  assert.deepEqual(texts, ["same text", "other text"]);
  assert.deepEqual(
    entries.map((e) => e.text),
    [0, 0, 1],
  );
});
