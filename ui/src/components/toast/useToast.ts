import { useMemo, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { toErrorPayload } from "../../api/ipc";
import { errorSentence } from "../../features/errors/errorText";
import i18n from "../../i18n";

export type ToastKind = "success" | "info" | "error";
export type ToastInput = { kind: ToastKind; message: string };
export type ToastItem = ToastInput & { id: number };

/** 同時に出すのは 3 つまで。超えたら古いものから消す */
export const MAX_TOASTS = 3;
/** 成功と情報は 5 秒、失敗は 8 秒で消える（マウスを乗せている間とフォーカスがある間は止まる） */
export const TOAST_TIMEOUT_MS: Record<ToastKind, number> = { success: 5000, info: 5000, error: 8000 };

// アプリに 1 つの並び。どの画面からでも出せるよう、React の外に置く
let toasts: ToastItem[] = [];
let nextId = 1;
const listeners = new Set<() => void>();

function emit() {
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function pushToast(toast: ToastInput): void {
  toasts = [...toasts, { ...toast, id: nextId++ }].slice(-MAX_TOASTS);
  emit();
}

export function dismissToast(id: number): void {
  if (!toasts.some((t) => t.id === id)) return;
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

/** テスト用。並びを空にする */
export function clearToasts(): void {
  toasts = [];
  emit();
}

export function useToastList(): ToastItem[] {
  return useSyncExternalStore(subscribe, () => toasts);
}

export function useToast(): { show(toast: ToastInput): void; showError(error: unknown): void } {
  const { t } = useTranslation();
  return useMemo(
    () => ({
      show: pushToast,
      showError: (error: unknown) =>
        pushToast({
          kind: "error",
          message: errorSentence(toErrorPayload(error), (key, options) => t(key as never, options), (key) => i18n.exists(key)),
        }),
    }),
    [t],
  );
}
