import { getCurrentWebview } from "@tauri-apps/api/webview";

export type DragDropHandlers = {
  enter(): void;
  leave(): void;
  drop(paths: string[]): void;
};

/**
 * ファイルのドラッグ＆ドロップを受ける。`enter` はファイルを持って入ったとき、`leave` は出たとき、
 * `drop` は落としたとき。戻り値は解除の関数。画面からはこの関数だけを使う。
 */
export async function onFilesDropped(handlers: DragDropHandlers): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") handlers.enter();
    else if (payload.type === "leave") handlers.leave();
    else if (payload.type === "drop") handlers.drop(payload.paths);
  });
}
