import type { ContentKind } from "../../bindings/ContentKind";

/** Modrinth で探せる中身の種類（コンテンツのサブタブと同じ並び）。 */
export const contentKinds: ContentKind[] = ["mod", "resourcepack", "shader"];

/** URL などから来た文字を種類に直す。知らない値と空は MOD。 */
export function parseKind(value: string | null | undefined): ContentKind {
  return contentKinds.find((k) => k === value) ?? "mod";
}
