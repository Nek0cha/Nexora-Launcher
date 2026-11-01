import type { LoaderKind } from "../../bindings/LoaderKind";
import { versionsFor } from "../instances/createLoader";

/**
 * 上げ先に選べる Minecraft バージョン。現在より新しい正式版のうち、ローダーが対応するものだけを新しい順に返す。
 * `vanilla` は新しい順の正式版。現在のバージョンがその中に無ければ空。
 */
export function upgradeTargets(
  current: string,
  loader: LoaderKind,
  vanilla: string[],
  loaderGameVersions: string[] | undefined,
): string[] {
  const position = vanilla.indexOf(current);
  if (position < 0) return [];
  return versionsFor(loader, vanilla.slice(0, position), loaderGameVersions);
}
