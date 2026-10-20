import type { TaskPhase } from "../../bindings/TaskPhase";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";

export type TaskTable = Record<string, TaskSnapshot>;

const TERMINAL = new Set<TaskSnapshot["state"]>(["succeeded", "failed", "cancelled"]);

export function isTerminal(snapshot: TaskSnapshot): boolean {
  return TERMINAL.has(snapshot.state);
}

/** 手元より新しい revision のときだけ反映する。変化がなければ同じオブジェクトを返す。 */
export function mergeSnapshot(table: TaskTable, snapshot: TaskSnapshot): TaskTable {
  const current = table[snapshot.id];
  if (current && current.revision >= snapshot.revision) return table;
  return { ...table, [snapshot.id]: snapshot };
}

/** 一覧を正として置き換える。ただし手元の方が新しいタスクは手元を残す。 */
export function mergeList(table: TaskTable, list: TaskSnapshot[], preserveIds: ReadonlySet<string> = new Set()): TaskTable {
  const next: TaskTable = {};
  for (const id of preserveIds) {
    if (table[id]) next[id] = table[id];
  }
  for (const snapshot of list) {
    const current = table[snapshot.id];
    next[snapshot.id] = current && current.revision > snapshot.revision ? current : snapshot;
  }
  return next;
}

export function removeTask(table: TaskTable, id: string): TaskTable {
  if (!(id in table)) return table;
  const { [id]: _removed, ...rest } = table;
  return rest;
}

export function activeCount(table: TaskTable): number {
  return Object.values(table).filter((t) => !isTerminal(t)).length;
}

export function sortedTasks(table: TaskTable): TaskSnapshot[] {
  return Object.values(table).sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}

/**
 * タスクの進み具合の見せ方。percent はバーを満たせる段階（確認と取得、段階の無いタスク）で全体が分かるときだけ、
 * count はローダーの構築（processor の数）で全体が分かるときだけ。どちらも null ならバーは進み具合を示さない。
 */
export type ProgressView = {
  phase: TaskPhase | null;
  percent: number | null;
  count: { done: number; total: number } | null;
};

export function progressView(task: TaskSnapshot): ProgressView {
  const { phase } = task;
  const { current, total } = task.progress;
  const known = total > 0;
  const fraction = phase === null || phase === "verifying" || phase === "downloading";
  return {
    phase,
    percent: fraction && known ? Math.min(100, Math.floor((current * 100) / total)) : null,
    count: phase === "building_loader" && known ? { done: Math.min(current, total), total } : null,
  };
}

const STEP_KEY = {
  resolving: "play.step.resolving",
  verifying: "play.step.verifying",
  downloading: "play.step.downloading",
  building_loader: "play.step.buildingLoader",
  finishing: "play.step.finishing",
  launching: "play.step.launching",
} as const satisfies Record<TaskPhase, string>;

export type PhaseLabel = { key: string; values: Record<string, number> };

/** 段階の名前の文言のキー（数を含まない）。 */
export function phaseKey(phase: TaskPhase): string {
  return STEP_KEY[phase];
}

/** 段階の名前と、意味のある数（確認と取得は %、ローダーの構築は「済んだ数/全体」）。段階が無ければ null。 */
export function phaseLabel(view: ProgressView): PhaseLabel | null {
  if (view.phase === null) return null;
  const key = STEP_KEY[view.phase];
  if (view.count) return { key: `${key}Count`, values: { done: view.count.done, total: view.count.total } };
  if (view.percent !== null) return { key: `${key}Percent`, values: { percent: view.percent } };
  return { key, values: {} };
}
