import assert from "node:assert/strict";
import { test } from "node:test";
import { checkVersions } from "./check-version.mjs";

const conf = (version) => JSON.stringify({ productName: "Nexora", version });
const cargo = (version) => `[workspace]\nmembers = []\n\n[workspace.package]\nversion = "${version}"\nedition = "2024"\n\n[profile.dev]\n`;

test("passes when all three match", () => {
  const r = checkVersions({ tauriConf: conf("0.1.0"), cargoToml: cargo("0.1.0"), tag: "v0.1.0" });
  assert.deepEqual(r, { ok: true, problems: [] });
});

test("passes without a tag when the files match", () => {
  assert.equal(checkVersions({ tauriConf: conf("0.1.0"), cargoToml: cargo("0.1.0") }).ok, true);
});

test("fails when tauri.conf.json differs", () => {
  const r = checkVersions({ tauriConf: conf("0.1.1"), cargoToml: cargo("0.1.0"), tag: "v0.1.0" });
  assert.equal(r.ok, false);
  assert.ok(r.problems.some((p) => p.includes("tauri.conf.json")));
});

test("fails when the tag differs", () => {
  const r = checkVersions({ tauriConf: conf("0.1.0"), cargoToml: cargo("0.1.0"), tag: "v0.2.0" });
  assert.equal(r.ok, false);
  assert.ok(r.problems.some((p) => p.includes("v0.2.0")));
});

test("fails when Cargo.toml has no workspace version", () => {
  const r = checkVersions({ tauriConf: conf("0.1.0"), cargoToml: "[workspace]\n" });
  assert.equal(r.ok, false);
});
