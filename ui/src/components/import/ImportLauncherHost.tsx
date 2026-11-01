import { useLauncherImportTaskWatch } from "../../features/import/useLauncherImport";
import { ImportLauncherDialog } from "./ImportLauncherDialog";

/** 他のランチャーからの取り込みのダイアログと、そのタスクの見届け。アプリで 1 回だけ置く。 */
export function ImportLauncherHost() {
  useLauncherImportTaskWatch();
  return <ImportLauncherDialog />;
}
