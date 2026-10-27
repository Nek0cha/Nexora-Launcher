import { type PointerEvent as ReactPointerEvent, useCallback, useEffect, useRef, useState } from "react";

/**
 * インスタンスのチップのドラッグ（共通設定の画面）。
 *
 * HTML5 の Drag and Drop は使わない。Tauri の dragDropEnabled（既定で有効。ファイルのドロップを
 * 受けるのに使っている）が有効だと、Windows の WebView2 では HTML5 のドラッグが届かないため、
 * ポインターのイベントで自前に動かし、離した位置の要素から落とし先を決める。
 */

/** 落とし先。カードは data-drop-zone="profile" と data-profile-id、未参加の一覧は data-drop-zone="unjoined" を持つ */
export type DropZone = { kind: "profile"; profileId: string } | { kind: "unjoined" } | { kind: "outside" };

/** ドラッグしているチップ。profileId は今参加している共通設定（未参加なら null） */
export type DragSource = { instanceId: string; profileId: string | null };

export type DropAction =
  | { kind: "join"; instanceId: string; profileId: string }
  | { kind: "leave"; instanceId: string }
  | { kind: "none" };

/** これだけ動くまではドラッグとみなさない（クリックや手の震えと区別する） */
export const DRAG_THRESHOLD_PX = 4;

export function passedDragThreshold(dx: number, dy: number): boolean {
  return Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX;
}

/** ポインターの下の要素から落とし先を決める。要素が無い（ページの外）なら null */
export function dropZoneAt(element: Element | null): DropZone | null {
  if (!element) return null;
  const zone = element.closest<HTMLElement>("[data-drop-zone]");
  if (!zone) return { kind: "outside" };
  if (zone.dataset.dropZone === "profile" && zone.dataset.profileId)
    return { kind: "profile", profileId: zone.dataset.profileId };
  if (zone.dataset.dropZone === "unjoined") return { kind: "unjoined" };
  return { kind: "outside" };
}

/** 落としたときの操作。カードへ落とせば参加（別のカードからなら移る）、カードの外へ落とせば離脱 */
export function dropAction(source: DragSource, zone: DropZone | null): DropAction {
  if (zone === null) return { kind: "none" };
  if (zone.kind === "profile") {
    if (zone.profileId === source.profileId) return { kind: "none" };
    return { kind: "join", instanceId: source.instanceId, profileId: zone.profileId };
  }
  return source.profileId === null ? { kind: "none" } : { kind: "leave", instanceId: source.instanceId };
}

export type DragState = DragSource & { name: string; x: number; y: number; zone: DropZone | null };

type Pending = DragSource & { name: string; pointerId: number; startX: number; startY: number; started: boolean };

/**
 * チップのドラッグの状態。start はチップの pointerdown から呼ぶ。
 * 離すと onDrop に落とし先の操作を渡す。Esc で取り消す。
 */
export function useChipDrag(
  onDrop: (action: DropAction) => void,
  hitTest: (x: number, y: number) => Element | null = (x, y) => document.elementFromPoint(x, y),
): { drag: DragState | null; start: (source: DragSource & { name: string }, event: ReactPointerEvent) => void } {
  const [drag, setDrag] = useState<DragState | null>(null);
  const pending = useRef<Pending | null>(null);
  const cleanup = useRef<(() => void) | null>(null);
  const latest = useRef({ onDrop, hitTest });
  latest.current = { onDrop, hitTest };

  const stop = useCallback(() => {
    cleanup.current?.();
    cleanup.current = null;
    pending.current = null;
    setDrag(null);
  }, []);

  useEffect(() => stop, [stop]);

  const start = useCallback(
    (source: DragSource & { name: string }, event: ReactPointerEvent) => {
      stop();
      pending.current = {
        ...source,
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        started: false,
      };

      const zoneAt = (x: number, y: number) => dropZoneAt(latest.current.hitTest(x, y));

      const move = (e: PointerEvent) => {
        const p = pending.current;
        if (!p || e.pointerId !== p.pointerId) return;
        // pointerup を取りこぼしたとき、ボタンが離れているのにゴーストが追従し続けないようにする
        if (e.buttons === 0) {
          stop();
          return;
        }
        if (!p.started && !passedDragThreshold(e.clientX - p.startX, e.clientY - p.startY)) return;
        p.started = true;
        document.body.style.cursor = "grabbing";
        setDrag({
          instanceId: p.instanceId,
          profileId: p.profileId,
          name: p.name,
          x: e.clientX,
          y: e.clientY,
          zone: zoneAt(e.clientX, e.clientY),
        });
      };
      const up = (e: PointerEvent) => {
        const p = pending.current;
        if (!p || e.pointerId !== p.pointerId) return;
        const action = p.started ? dropAction(p, zoneAt(e.clientX, e.clientY)) : ({ kind: "none" } as const);
        stop();
        if (action.kind !== "none") latest.current.onDrop(action);
      };
      const cancel = (e: PointerEvent) => {
        if (pending.current && e.pointerId === pending.current.pointerId) stop();
      };
      const key = (e: KeyboardEvent) => {
        if (e.key === "Escape") stop();
      };

      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", cancel);
      window.addEventListener("keydown", key);
      cleanup.current = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", cancel);
        window.removeEventListener("keydown", key);
        document.body.style.cursor = "";
      };
    },
    [stop],
  );

  return { drag, start };
}
