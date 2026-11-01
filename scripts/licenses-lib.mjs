// 第三者のライセンスの一覧を作るための、副作用のない部品。

/** 配布物に含めてよいライセンス（SPDX の識別子）。ここに無いものが依存に混ざったら、一覧の生成を失敗させる */
export const ACCEPTED = new Set([
  "MIT",
  "MIT-0",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "MPL-2.0",
  "Zlib",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "0BSD",
  "BSL-1.0",
  "Unlicense",
  "OFL-1.1",
  "BlueOak-1.0.0",
  "CC-BY-4.0",
  "Python-2.0",
  "CDLA-Permissive-2.0",
]);

/**
 * SPDX 式が許可されたライセンスだけで満たせるか。「A OR B」はどちらかが許可されていればよく、
 * 「A AND B」は両方が必要。WITH の例外（LLVM-exception など）は本体のライセンスだけを見る。
 */
export function isAccepted(expression, accepted = ACCEPTED) {
  const normalized = expression
    .replace(/\//g, " OR ")
    .replace(/[()]/g, " ")
    .replace(/\s+WITH\s+\S+/g, "")
    .trim();
  if (!normalized) return false;
  return normalized.split(/\s+OR\s+/).some((alternative) =>
    alternative
      .split(/\s+AND\s+/)
      .map((id) => id.trim())
      .every((id) => accepted.has(id)),
  );
}

/**
 * 同じ本文のライセンスをまとめて、読みやすい 1 つの文章にする。
 * packages: { name, version, license, repository?, text }[]
 */
export function renderNotices(packages) {
  const groups = new Map();
  for (const pkg of [...packages].sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version))) {
    const key = pkg.text.trim();
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(pkg);
  }
  const parts = [
    "Nexora Launcher is licensed under the Apache License 2.0. It bundles the following third-party software.",
  ];
  for (const [text, members] of [...groups].sort((a, b) => b[1].length - a[1].length)) {
    const names = members.map((m) => `${m.name} ${m.version} (${m.license})`).join("\n");
    parts.push(`${"=".repeat(72)}\n${names}\n${"-".repeat(72)}\n${text}`);
  }
  return parts.join("\n\n") + "\n";
}
