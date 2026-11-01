import { useSyncExternalStore } from "react";
import { checkUpdate, restartApp, type AvailableUpdate } from "../../api/updater";
import { pushToast } from "../../components/toast/useToast";
import i18n from "../../i18n";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; version: string }
  | { kind: "downloading"; percent: number | null }
  | { kind: "failed" };

// アプリに 1 つの状態。ヘッダーの更新ボタンと設定画面が共有するため、React の外に置く
const IDLE: UpdateState = { kind: "idle" };
let state: UpdateState = IDLE;
let pending: AvailableUpdate | null = null;
let busy = false;
const listeners = new Set<() => void>();

function set(next: UpdateState) {
  state = next;
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function useUpdateState(): UpdateState {
  return useSyncExternalStore(subscribe, () => state);
}

/**
 * 更新を確認する。起動時の確認（manual: false）は、結果が無くても失敗しても何も見せない。
 * 手動の確認（manual: true）は、確認中、最新、失敗を状態に出す。
 */
export async function checkForUpdate({ manual }: { manual: boolean }): Promise<void> {
  if (busy) return;
  busy = true;
  if (manual) set({ kind: "checking" });
  try {
    const found = await checkUpdate();
    pending = found;
    if (found) set({ kind: "available", version: found.version });
    else set(manual ? { kind: "upToDate" } : IDLE);
  } catch {
    pending = null;
    set(manual ? { kind: "failed" } : IDLE);
  } finally {
    busy = false;
  }
}

/** ダウンロードしてインストールし、再起動する。失敗したら、トーストで知らせて元の状態に戻す */
export async function installUpdate(): Promise<void> {
  const update = pending;
  if (!update || busy) return;
  busy = true;
  set({ kind: "downloading", percent: null });
  try {
    await update.install(({ downloaded, total }) =>
      set({
        kind: "downloading",
        percent: total ? Math.min(100, Math.round((downloaded / total) * 100)) : null,
      }),
    );
    await restartApp();
  } catch {
    set({ kind: "available", version: update.version });
    pushToast({ kind: "error", message: i18n.t("update.failed") });
  } finally {
    busy = false;
  }
}

/** テスト用 */
export function resetUpdateStore(): void {
  state = IDLE;
  pending = null;
  busy = false;
  for (const fn of listeners) fn();
}
