// 配布物に含まれる第三者のライセンスを集めて、ui/src/assets/third-party-licenses.txt に書く。
//   node scripts/gen-licenses.mjs
// Rust は cargo metadata、フロントエンドは pnpm licenses から、依存の名前、バージョン、ライセンス、本文を集める。
// 許可していないライセンスの依存があれば、何も書かずに失敗する（許可の一覧は licenses-lib.mjs）。
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isAccepted, renderNotices } from "./licenses-lib.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(root, "ui", "src", "assets", "third-party-licenses.txt");
// 開発用の依存でも、ビルドの結果に取り込まれるもの（アイコンの図形）
const BUNDLED_DEV_NPM = ["@iconify-json/ph"];
const LICENSE_FILE = /^(licen[sc]e|copying|notice|unlicense)([-._].*)?$/i;

function run(cmd, args, cwd) {
  return execFileSync(cmd, args, { cwd, encoding: "utf8", maxBuffer: 512 * 1024 * 1024, shell: process.platform === "win32" });
}

function licenseText(dir) {
  const files = readdirSync(dir).filter((f) => LICENSE_FILE.test(f)).sort();
  return files.map((f) => readFileSync(join(dir, f), "utf8").trim()).join("\n\n");
}

// 配布する 3 つの OS。それぞれの依存の和集合を取る（Android など配布しない環境の依存を除く）
const TRIPLES = ["x86_64-pc-windows-msvc", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu"];

function rustPackagesFor(triple) {
  const meta = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--locked", "--filter-platform", triple], root));
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const workspace = new Set(meta.workspace_members);
  const app = meta.packages.find((p) => p.name === "nexora");
  // アプリから、通常の依存だけをたどる（ビルド用と開発用は配布物に入らない）
  const seen = new Set();
  const queue = [app.id];
  while (queue.length > 0) {
    const id = queue.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((k) => k.kind === null)) queue.push(dep.pkg);
    }
  }
  return [...seen].filter((id) => !workspace.has(id)).map((id) => byId.get(id));
}

function rustPackages() {
  const unique = new Map();
  for (const triple of TRIPLES) {
    for (const p of rustPackagesFor(triple)) unique.set(`${p.name}@${p.version}`, p);
  }
  return [...unique.values()].map((p) => ({
    name: p.name,
    version: p.version,
    license: p.license ?? "UNKNOWN",
    // クレートにライセンスの本文が無いときは、宣言されたライセンスと配布元だけを書く
    text:
      licenseText(dirname(p.manifest_path)) ||
      `This package does not include a license file. Declared license: ${p.license ?? "UNKNOWN"}.${p.repository ? ` Source: ${p.repository}` : ""}`,
  }));
}

function npmPackages() {
  const grouped = JSON.parse(run("pnpm", ["licenses", "list", "--prod", "--json"], join(root, "ui")));
  const found = [];
  for (const [license, list] of Object.entries(grouped)) {
    for (const pkg of list) {
      pkg.versions.forEach((version, i) => found.push({ name: pkg.name, version, license, dir: pkg.paths[i] }));
    }
  }
  for (const name of BUNDLED_DEV_NPM) {
    const dir = join(root, "ui", "node_modules", ...name.split("/"));
    const manifest = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    found.push({ name, version: manifest.version, license: manifest.license ?? "UNKNOWN", dir });
  }
  return found
    .filter((p) => !p.name.startsWith("@tauri-apps/") || existsSync(p.dir))
    .map((p) => ({ name: p.name, version: p.version, license: p.license, text: licenseText(p.dir) }));
}

const packages = [...rustPackages(), ...npmPackages()];
const rejected = packages.filter((p) => !isAccepted(p.license));
if (rejected.length > 0) {
  console.error("Licenses that are not accepted:");
  for (const p of rejected) console.error(`  ${p.name} ${p.version}: ${p.license}`);
  process.exit(1);
}
writeFileSync(OUT, renderNotices(packages));
console.log(`Wrote ${packages.length} packages to ${OUT}`);
