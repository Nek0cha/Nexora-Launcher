import type { LoaderKind } from "../../bindings/LoaderKind";

/** 名前の既定値。vanilla はバージョン名、それ以外は「ローダー名 バージョン名」。 */
export function defaultName(loader: LoaderKind, mcVersion: string, loaderLabel: string): string {
  return loader === "vanilla" ? mcVersion : `${loaderLabel} ${mcVersion}`;
}

/** 既定の名前が変わったときの名前。空、または前の既定の名前のままなら、新しい既定の名前にする。 */
export function nextName(current: string, previousDefault: string | null, nextDefault: string): string {
  const trimmed = current.trim();
  if (trimmed === "" || trimmed === previousDefault) return nextDefault;
  return current;
}
