import { useQuery } from "@tanstack/react-query";
import { listGameLogs, readGameLog } from "../../api/commands";

/** 起動中のゲームのログを追いかける間隔 */
export const LIVE_INTERVAL_MS = 2000;

/** 起動の一覧。新しい起動はゲームが始まってから増えるので、起動中は追いかける */
export function useGameLogs(instanceId: string, live: boolean) {
  return useQuery({
    queryKey: ["game-logs", instanceId],
    queryFn: () => listGameLogs(instanceId),
    refetchInterval: live ? LIVE_INTERVAL_MS : false,
  });
}

/** ログの末尾。書き込み中のログ（live）は一定の間隔で読み直す */
export function useGameLog(instanceId: string, name: string | null, live: boolean) {
  return useQuery({
    queryKey: ["game-log", instanceId, name],
    queryFn: () => readGameLog(instanceId, name as string),
    enabled: name !== null,
    refetchInterval: live ? LIVE_INTERVAL_MS : false,
  });
}
