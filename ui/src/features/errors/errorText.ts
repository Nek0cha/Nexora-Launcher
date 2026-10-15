/** errors 名前空間の翻訳キーを返す。未知のコードは汎用の文にする。 */
export function errorKey(code: string, exists: (key: string) => boolean): string {
  const key = `errors:${code}`;
  return exists(key) ? key : "errors:generic";
}
