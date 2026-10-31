export type LogLevel = "error" | "warn" | "other";

export type LogLine = { text: string; level: LogLevel };

/** 表示の絞り込み。all は全部、warn は警告とエラー、error はエラーだけ */
export type LevelFilter = "all" | "warn" | "error";

// ファイルの行は「[12:34:56 UTC] [Render thread/INFO]: …」。古い版の形（UTC なし）も受ける
const LEVEL = /^\[[^\]]*\]\s*\[[^\]]*\/(FATAL|ERROR|WARN|WARNING|INFO|DEBUG|TRACE)\]/;
// スタックトレースの続き。直前の行のレベルを引き継ぐ
const CONTINUATION = /^(\s|Caused by:|[\w.$]+(Exception|Error)\b)/;

function levelOf(name: string): LogLevel {
  if (name === "FATAL" || name === "ERROR") return "error";
  if (name === "WARN" || name === "WARNING") return "warn";
  return "other";
}

/** ログの本文を行に分け、行ごとのレベルを付ける。続きの行（スタックトレースなど）は直前の行のレベルになる。 */
export function parseLog(text: string): LogLine[] {
  const rows = text.split("\n").map((row) => row.replace(/\r$/, ""));
  if (rows.at(-1) === "") rows.pop();
  const lines: LogLine[] = [];
  let previous: LogLevel = "other";
  for (const row of rows) {
    const match = LEVEL.exec(row);
    let level: LogLevel;
    if (match) level = levelOf(match[1] as string);
    else if (CONTINUATION.test(row)) level = previous;
    else level = "other";
    previous = level;
    lines.push({ text: row, level });
  }
  return lines;
}

const rank: Record<LogLevel, number> = { other: 0, warn: 1, error: 2 };
const minimum: Record<LevelFilter, number> = { all: 0, warn: 1, error: 2 };

/** レベルと検索語（大文字小文字を区別しない）で絞り込む。 */
export function filterLines(lines: LogLine[], level: LevelFilter, query: string): LogLine[] {
  const needle = query.trim().toLowerCase();
  if (level === "all" && needle === "") return lines;
  return lines.filter(
    (line) => rank[line.level] >= minimum[level] && (needle === "" || line.text.toLowerCase().includes(needle)),
  );
}
