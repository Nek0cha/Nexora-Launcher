import type { ErrorPayload } from "../../bindings/ErrorPayload";

/** errors 名前空間の翻訳キーを返す。未知のコードは汎用の文にする。 */
export function errorKey(code: string, exists: (key: string) => boolean): string {
  const key = `errors:${code}`;
  return exists(key) ? key : "errors:generic";
}

/** auth.xbox は XErr の番号ごとの文言があればそれを使い、なければ番号つきの汎用文にする。 */
export function noticeKey(error: ErrorPayload, exists: (key: string) => boolean): string {
  const xerr = error.params.xerr;
  if (error.code === "auth.xbox" && xerr !== undefined) {
    const specific = `errors:auth.xbox_${xerr}`;
    if (exists(specific)) return specific;
  }
  return errorKey(error.code, exists);
}

type Translate = (key: string, options: Record<string, unknown>) => string;

/** 失敗を 1 つの文にする。その場の表示（FailureNotice）とトーストで同じ文を使う。 */
export function errorSentence(error: ErrorPayload | null, t: Translate, exists: (key: string) => boolean): string {
  if (!error) return t("errors:generic", { code: "unknown" });
  return t(noticeKey(error, exists), { ...(error.params as Record<string, unknown>), code: error.code });
}
