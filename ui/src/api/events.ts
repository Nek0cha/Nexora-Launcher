import { listen } from "@tauri-apps/api/event";
import type { TaskChangedEvent } from "../bindings/TaskChangedEvent";
import type { GameExitedEvent } from "../bindings/GameExitedEvent";
import type { LoginProgressEvent } from "../bindings/LoginProgressEvent";
import type { RunningSession } from "../bindings/RunningSession";
import type { SyncApplyFinishedEvent } from "../bindings/SyncApplyFinishedEvent";

export const TASK_CHANGED = "task://changed";

export async function onTaskChanged(handler: (event: TaskChangedEvent) => void): Promise<() => void> {
  return listen<TaskChangedEvent>(TASK_CHANGED, (e) => handler(e.payload));
}

export const GAME_STARTED = "game://started";
export const GAME_EXITED = "game://exited";
export const GAME_LOG = "game://log";

export async function onGameStarted(handler: (event: RunningSession) => void): Promise<() => void> {
  return listen<RunningSession>(GAME_STARTED, (e) => handler(e.payload));
}

export async function onGameExited(handler: (event: GameExitedEvent) => void): Promise<() => void> {
  return listen<GameExitedEvent>(GAME_EXITED, (e) => handler(e.payload));
}

export const ACCOUNTS_CHANGED = "accounts://changed";
export const LOGIN_PROGRESS = "login://progress";

export async function onAccountsChanged(handler: () => void): Promise<() => void> {
  return listen(ACCOUNTS_CHANGED, () => handler());
}

export const SYNC_APPLY_FINISHED = "sync-apply-finished";

/** 参加中のインスタンスの、起動前の共通設定の適用が終わったとき。 */
export async function onSyncApplyFinished(handler: (event: SyncApplyFinishedEvent) => void): Promise<() => void> {
  return listen<SyncApplyFinishedEvent>(SYNC_APPLY_FINISHED, (e) => handler(e.payload));
}

export async function onLoginProgress(handler: (event: LoginProgressEvent) => void): Promise<() => void> {
  return listen<LoginProgressEvent>(LOGIN_PROGRESS, (e) => handler(e.payload));
}
