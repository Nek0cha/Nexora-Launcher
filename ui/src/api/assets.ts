import { convertFileSrc } from "@tauri-apps/api/core";

/** ディスク上の画像を <img> に出せる URL にする（Tauri の asset プロトコル）。画面からはこの関数だけを使う */
export const fileUrl = (path: string): string => convertFileSrc(path);
