import type { LoaderKind } from "../../bindings/LoaderKind";

/** タブの表示順。 */
export const LOADER_TABS: readonly LoaderKind[] = ["vanilla", "fabric", "forge", "neoforge", "quilt"];

export type TabState = { enabled: true } | { enabled: false; reason: "noVersions" | "loading" };

export type GameVersions = Partial<Record<LoaderKind, string[] | undefined>>;

/** タブを選べるか。知っている正式版のうち 1 つでも対応していれば選べる。選べない場合はその理由も返す。 */
export function tabState(loader: LoaderKind, vanilla: string[], gameVersions: GameVersions): TabState {
  if (loader === "vanilla") return { enabled: true };
  const list = gameVersions[loader];
  if (list === undefined) return { enabled: false, reason: "loading" };
  if (versionsFor(loader, vanilla, list).length === 0) return { enabled: false, reason: "noVersions" };
  return { enabled: true };
}

/** そのローダーで選べる Minecraft バージョン。vanilla の並びを保つ。 */
export function versionsFor(loader: LoaderKind, vanilla: string[], gameVersions: string[] | undefined): string[] {
  if (loader === "vanilla") return vanilla;
  if (gameVersions === undefined) return [];
  const supported = new Set(gameVersions);
  return vanilla.filter((v) => supported.has(v));
}

/** ローダーのタブに切り替えたときの Minecraft バージョン。対応していれば今のまま、無ければ対応する最新。 */
export function versionForLoader(loader: LoaderKind, current: string, vanilla: string[], gameVersions: GameVersions): string | null {
  if (loader === "vanilla") return current;
  const supported = versionsFor(loader, vanilla, gameVersions[loader]);
  return supported.includes(current) ? current : (supported[0] ?? null);
}

/** 一覧が取り直されるなどで現在のタブが今のバージョンに対応しなくなったら vanilla に戻す。 */
export function fallbackLoader(current: LoaderKind, mcVersion: string, vanilla: string[], gameVersions: GameVersions): LoaderKind {
  return versionForLoader(current, mcVersion, vanilla, gameVersions) === mcVersion ? current : "vanilla";
}

/** ローダーのバージョンの選び方。現在の選択を残し、無ければ安定版の先頭、それも無ければ先頭。 */
export function pickLoaderVersion(entries: { version: string; stable: boolean }[], current: string | null): string | null {
  if (current !== null && entries.some((e) => e.version === current)) return current;
  const stable = entries.find((e) => e.stable);
  return (stable ?? entries[0])?.version ?? null;
}
