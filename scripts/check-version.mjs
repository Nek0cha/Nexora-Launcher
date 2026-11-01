// tauri.conf.json、workspace の Cargo.toml、リリースのタグの版番号が一致しているかを調べる。
//   node scripts/check-version.mjs            （ファイル同士を比べる）
//   GITHUB_REF_NAME=v0.1.0 node scripts/check-version.mjs   （タグとも比べる）
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** workspace の Cargo.toml の [workspace.package] にある version を取り出す */
function cargoVersion(cargoToml) {
  const section = cargoToml.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/);
  const version = section?.[1].match(/^\s*version\s*=\s*"([^"]+)"/m);
  return version ? version[1] : null;
}

export function checkVersions({ tauriConf, cargoToml, tag }) {
  const problems = [];
  const tauri = JSON.parse(tauriConf).version ?? null;
  const cargo = cargoVersion(cargoToml);
  if (!tauri) problems.push("tauri.conf.json has no version");
  if (!cargo) problems.push("Cargo.toml has no [workspace.package] version");
  if (tauri && cargo && tauri !== cargo) {
    problems.push(`tauri.conf.json (${tauri}) differs from Cargo.toml (${cargo})`);
  }
  if (tag) {
    const tagged = tag.replace(/^v/, "");
    if (tauri && tagged !== tauri) problems.push(`the tag ${tag} differs from tauri.conf.json (${tauri})`);
    if (cargo && tagged !== cargo) problems.push(`the tag ${tag} differs from Cargo.toml (${cargo})`);
  }
  return { ok: problems.length === 0, problems };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const ref = process.env.GITHUB_REF_NAME;
  const result = checkVersions({
    tauriConf: readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"),
    cargoToml: readFileSync(join(root, "Cargo.toml"), "utf8"),
    // ブランチの push では比べない。タグ（v で始まる名前）のときだけ比べる
    tag: ref && /^v\d/.test(ref) ? ref : undefined,
  });
  if (!result.ok) {
    for (const problem of result.problems) console.error(problem);
    process.exit(1);
  }
  console.log("Versions match.");
}
