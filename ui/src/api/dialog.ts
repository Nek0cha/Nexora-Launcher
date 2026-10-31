import { open, save } from "@tauri-apps/plugin-dialog";

const MRPACK_FILTER = { name: "mrpack", extensions: ["mrpack"] };

/** `.mrpack` を 1 つ選ぶ。キャンセルしたら null。画面からはこの関数だけを使う。 */
export async function pickMrpack(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false, filters: [MRPACK_FILTER] });
  return typeof picked === "string" ? picked : null;
}

/** `.mrpack` の保存先を選ぶ。キャンセルしたら null。 */
export async function pickMrpackSavePath(defaultName: string): Promise<string | null> {
  return save({ defaultPath: defaultName, filters: [MRPACK_FILTER] });
}
