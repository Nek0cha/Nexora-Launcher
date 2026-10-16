import { listen } from "@tauri-apps/api/event";
import type { TaskChangedEvent } from "../bindings/TaskChangedEvent";
import type { GameExitedEvent } from "../bindings/GameExitedEvent";
import type { RunningSession } from "../bindings/RunningSession";

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
