import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export type DownloadProgress = { downloaded: number; total: number | null };

export type AvailableUpdate = {
  version: string;
  /** ダウンロードしてインストールする。再起動はしない（`restartApp` で行う） */
  install(onProgress: (progress: DownloadProgress) => void): Promise<void>;
};

/** 新しいバージョンがあればそれを返す。無ければ null。確認に失敗したときは投げる */
export async function checkUpdate(): Promise<AvailableUpdate | null> {
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    install: async (onProgress) => {
      let downloaded = 0;
      let total: number | null = null;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          onProgress({ downloaded, total });
        }
      });
    },
  };
}

export function restartApp(): Promise<void> {
  return relaunch();
}
