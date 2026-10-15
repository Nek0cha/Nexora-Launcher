import { listen } from "@tauri-apps/api/event";
import type { TaskChangedEvent } from "../bindings/TaskChangedEvent";

export const TASK_CHANGED = "task://changed";

export async function onTaskChanged(handler: (event: TaskChangedEvent) => void): Promise<() => void> {
  return listen<TaskChangedEvent>(TASK_CHANGED, (e) => handler(e.payload));
}
