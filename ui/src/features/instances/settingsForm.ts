import type { Instance } from "../../bindings/Instance";
import type { InstanceSettings } from "../../bindings/InstanceSettings";
import type { JavaMode } from "../../bindings/JavaMode";
import type { MemoryMode } from "../../bindings/MemoryMode";

/** 手動で指定できるメモリの下限と上限（MB）。core の MEMORY_MIN_MB / MEMORY_MAX_MB と同じ値である。 */
export const MEMORY_MIN_MB = 512;
export const MEMORY_MAX_MB = 65536;
/** 手動に切り替えたときの最初のメモリ量（MB）。 */
export const MEMORY_DEFAULT_MB = 4096;

/**
 * 設定のフォームの入力値。自動のときも手動用の値を持ち続け、自動と手動を行き来しても入力が消えないようにする。
 * 保存のときに toSettings で backend の形にそろえる。
 */
export type SettingsForm = {
  name: string;
  memoryMode: MemoryMode;
  memoryMb: number;
  javaMode: JavaMode;
  javaPath: string;
  jvmArgs: string;
};

export function toForm(i: Instance): SettingsForm {
  return {
    name: i.name,
    memoryMode: i.memoryMode,
    memoryMb: i.memoryMb ?? MEMORY_DEFAULT_MB,
    javaMode: i.javaMode,
    javaPath: i.javaPath ?? "",
    jvmArgs: i.jvmArgs,
  };
}

/**
 * Java のパスを整える。前後の空白を除き、両端の二重引用符を 1 組だけ外す。
 * Windows の「パスのコピー」は `"C:\...\java.exe"` の形になり、backend は引用符を外さないためである。
 */
function normalizeJavaPath(path: string): string {
  const trimmed = path.trim();
  if (trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')) return trimmed.slice(1, -1).trim();
  return trimmed;
}

/** 保存する値。自動のときは手動用の値を null にする。 */
export function toSettings(f: SettingsForm): InstanceSettings {
  return {
    name: f.name.trim(),
    memoryMode: f.memoryMode,
    memoryMb: f.memoryMode === "manual" ? f.memoryMb : null,
    javaMode: f.javaMode,
    javaPath: f.javaMode === "manual" ? normalizeJavaPath(f.javaPath) : null,
    jvmArgs: f.jvmArgs.trim(),
  };
}

/** 保存される値が違うか。自動のときの手動用の値や前後の空白だけの違いは、変更とみなさない。 */
export function isDirty(a: SettingsForm, b: SettingsForm): boolean {
  const x = toSettings(a);
  const y = toSettings(b);
  return (Object.keys(x) as (keyof InstanceSettings)[]).some((k) => x[k] !== y[k]);
}

/**
 * core の jvm_args::split と同じ規則で区切る。空白で区切り、`"` で囲んだ部分は空白を含めて 1 つの引数にする。
 * 閉じていない `"` は末尾までを 1 つの引数とみなす。引用符そのものは含めない。
 */
function splitArgs(input: string): string[] {
  const out: string[] = [];
  let current = "";
  let quoted = false;
  for (const c of input) {
    if (c === '"') quoted = !quoted;
    else if (/\s/.test(c) && !quoted) {
      if (current) out.push(current);
      current = "";
    } else current += c;
  }
  if (current) out.push(current);
  return out;
}

/** JVM 引数に `-Xmx` で始まる引数があるか。起動のときに core が取り除き、メモリの設定を使う。 */
export function hasMemoryFlag(jvmArgs: string): boolean {
  return splitArgs(jvmArgs).some((a) => a.startsWith("-Xmx"));
}

/** 手動のメモリ量が範囲の外なら "range" を返す。自動のときは検証しない。 */
export function memoryError(f: SettingsForm): "range" | null {
  if (f.memoryMode !== "manual") return null;
  const mb = f.memoryMb;
  return Number.isInteger(mb) && mb >= MEMORY_MIN_MB && mb <= MEMORY_MAX_MB ? null : "range";
}

/** 保存できる入力か。名前が空白だけ、手動のメモリが範囲の外、手動の Java でパスが空のときは保存できない。 */
export function isValid(f: SettingsForm): boolean {
  const s = toSettings(f);
  if (s.name === "") return false;
  if (memoryError(f)) return false;
  if (s.javaMode === "manual" && !s.javaPath) return false;
  return true;
}
