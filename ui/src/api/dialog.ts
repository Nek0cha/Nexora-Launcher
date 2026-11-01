import { open, save } from "@tauri-apps/plugin-dialog";
import type { ContentKind } from "../bindings/ContentKind";

const MRPACK_FILTER = { name: "mrpack", extensions: ["mrpack"] };

/** `.mrpack` を 1 つ選ぶ。キャンセルしたら null。画面からはこの関数だけを使う。 */
export async function pickMrpack(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false, filters: [MRPACK_FILTER] });
  return typeof picked === "string" ? picked : null;
}

const CONTENT_FILTERS: Record<ContentKind, { name: string; extensions: string[] }> = {
  mod: { name: "jar", extensions: ["jar"] },
  resourcepack: { name: "zip", extensions: ["zip"] },
  shader: { name: "zip", extensions: ["zip"] },
};

/** コンテンツとして追加するファイルを選ぶ（複数可）。キャンセルしたら空の配列。 */
export async function pickContentFiles(kind: ContentKind): Promise<string[]> {
  const picked = await open({ multiple: true, directory: false, filters: [CONTENT_FILTERS[kind]] });
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}

/** フォルダを 1 つ選ぶ。キャンセルしたら null。 */
export async function pickFolder(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: true });
  return typeof picked === "string" ? picked : null;
}

/** `.mrpack` の保存先を選ぶ。キャンセルしたら null。 */
export async function pickMrpackSavePath(defaultName: string): Promise<string | null> {
  return save({ defaultPath: defaultName, filters: [MRPACK_FILTER] });
}
