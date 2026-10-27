import type { Applicability } from "../../bindings/Applicability";
import type { Category } from "../../bindings/Category";
import type { SyncField } from "../../bindings/SyncField";

/** 既存の名前と重ならない既定の名前。重なるときは「名前 2」から空いている番号を付ける。 */
export function defaultProfileName(base: string, taken: string[]): string {
  const names = new Set(taken.map((n) => n.trim()));
  if (!names.has(base)) return base;
  for (let i = 2; ; i++) {
    const candidate = `${base} ${i}`;
    if (!names.has(candidate)) return candidate;
  }
}

/**
 * 登録表にある項目か、Mod が足したキー割り当て（core の field_for_key と同じ規則）か。
 * 共通設定の変更のコマンドには、これに当てはまる ID だけを送る。
 */
export function isKnownFieldId(id: string, fields: SyncField[]): boolean {
  if (fields.some((f) => f.id === id)) return true;
  return id.length > "key_".length && id.startsWith("key_") && !/[\n\r:]/.test(id);
}

/** 項目の種別。登録表に無いキー割り当てはキー割り当ての種別に入れる */
export function fieldCategory(id: string, fields: SyncField[]): Category | null {
  const found = fields.find((f) => f.id === id);
  if (found) return found.category;
  return isKnownFieldId(id, fields) ? "keys" : null;
}

const REASONS = [
  "noOptionsFile",
  "noDataVersion",
  "versionMismatch",
  "nothingToApply",
  "readFailed",
  "writeFailed",
  "recordFailed",
  "unexpected",
] as const;
type Reason = (typeof REASONS)[number];

/** スキップや失敗の理由の分類を文言のキーにする。知らない分類は「予期しない問題」として出す */
export function reasonKey(reason: string | null): `sync.reason.${Reason}` {
  const known = REASONS.find((r) => r === reason);
  return `sync.reason.${known ?? "unexpected"}`;
}

export function applicabilityKey(
  applicability: Exclude<Applicability, "ready">,
): `sync.applicability.${Exclude<Applicability, "ready">}` {
  return `sync.applicability.${applicability}`;
}
