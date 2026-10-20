import type { LoaderKind } from "../../bindings/LoaderKind";

/** タブの表示順。 */
export const LOADER_TABS: readonly LoaderKind[] = ["vanilla", "fabric", "forge", "neoforge", "quilt"];

export type TabState = { enabled: true } | { enabled: false; reason: "soon" | "unsupportedVersion" | "loading" };

type GameVersions = Partial<Record<LoaderKind, string[] | undefined>>;

/** タブを選べるか。選べない場合はその理由も返す。 */
export function tabState(loader: LoaderKind, mcVersion: string | null, gameVersions: GameVersions): TabState {
  if (loader === "vanilla") return { enabled: true };
  // Forge と NeoForge はこのフェーズでは未対応。
  if (loader === "forge" || loader === "neoforge") return { enabled: false, reason: "soon" };
  const list = gameVersions[loader];
  if (list === undefined) return { enabled: false, reason: "loading" };
  if (mcVersion === null || !list.includes(mcVersion)) return { enabled: false, reason: "unsupportedVersion" };
  return { enabled: true };
}

/** そのローダーで選べる Minecraft バージョン。vanilla の並びを保つ。 */
export function versionsFor(loader: LoaderKind, vanilla: string[], gameVersions: string[] | undefined): string[] {
  if (loader === "vanilla") return vanilla;
  if (gameVersions === undefined) return [];
  const supported = new Set(gameVersions);
  return vanilla.filter((v) => supported.has(v));
}

/** バージョンを変えて現在のタブが選べなくなったら vanilla に戻す。 */
export function fallbackLoader(current: LoaderKind, mcVersion: string, gameVersions: GameVersions): LoaderKind {
  return tabState(current, mcVersion, gameVersions).enabled ? current : "vanilla";
}

/** ローダーのバージョンの選び方。現在の選択を残し、無ければ安定版の先頭、それも無ければ先頭。 */
export function pickLoaderVersion(entries: { version: string; stable: boolean }[], current: string | null): string | null {
  if (current !== null && entries.some((e) => e.version === current)) return current;
  const stable = entries.find((e) => e.stable);
  return (stable ?? entries[0])?.version ?? null;
}
