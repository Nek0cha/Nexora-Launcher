import { useRunning } from "../play/useGameSync";
import { installUpdate } from "./updateStore";

/** 更新のインストールを始められるか。ゲームの起動中と、起動中かどうかが分からない間は始められない */
export function useUpdateInstall(): { canInstall: boolean; install: () => Promise<void> } {
  const running = useRunning();
  return { canInstall: running !== undefined && running.length === 0, install: installUpdate };
}
