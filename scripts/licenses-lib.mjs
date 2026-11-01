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
 * 画面に出すための形にする。パッケージを名前順に並べ、同じ本文は 1 つにまとめて番号で指す
 * （本文全体を 1 つの文章にして画面に出すと、数万行になって固まるため）。
 * packages: { name, version, license, text }[]
 * 戻り値: { entries: { name, version, license, text: number }[], texts: string[] }
 */
export function buildNotices(packages) {
  const texts = [];
  const index = new Map();
  const entries = [...packages]
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version))
    .map(({ name, version, license, text }) => {
      const key = text.trim();
      if (!index.has(key)) {
        index.set(key, texts.length);
        texts.push(key);
      }
      return { name, version, license, text: index.get(key) };
    });
  return { entries, texts };
}
