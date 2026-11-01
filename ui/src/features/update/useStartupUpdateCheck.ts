import { useEffect, useRef } from "react";
import type { AppInfo } from "../../bindings/AppInfo";
import { checkForUpdate } from "./updateStore";

/** 起動後に 1 度だけ、黙って更新を確認する。開発ビルドと、自己更新できない環境では確認しない */
export function useStartupUpdateCheck(info: AppInfo | undefined): void {
  const done = useRef(false);
  useEffect(() => {
    if (done.current || !info) return;
    done.current = true;
    if (info.build === "release" && info.selfUpdate) void checkForUpdate({ manual: false });
  }, [info]);
}
