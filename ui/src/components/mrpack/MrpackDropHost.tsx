import { useEffect } from "react";
import { onFilesDropped } from "../../api/dragDrop";
import { isMrpackPath } from "../../features/mrpack/mrpackPath";
import { openMrpackImport, useMrpackTaskWatch } from "../../features/mrpack/useMrpackImport";
import { ImportMrpackDialog } from "./ImportMrpackDialog";

/**
 * ウィンドウ全体で `.mrpack` のドロップを受け、読み込みの確認ダイアログを開く。アプリで 1 回だけ置く。
 * ほかの種類のファイルは何もしない（コンテンツやスキンなど、画面ごとの受け口に任せる）。
 */
export function MrpackDropHost() {
  useMrpackTaskWatch();
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void onFilesDropped({
      enter() {},
      leave() {},
      drop(paths) {
        const pack = paths.find(isMrpackPath);
        if (pack !== undefined) openMrpackImport(pack);
      },
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return <ImportMrpackDialog />;
}
