import { useCallback, useRef } from "react";
import { useLocation, useNavigate, useNavigationType } from "react-router";

export type HistoryNav = {
  canGoBack: boolean;
  canGoForward: boolean;
  back(): void;
  forward(): void;
};

/** ハッシュルーターが history.state に入れる、履歴の中の位置 */
function currentIdx(): number {
  const state: unknown = window.history.state;
  if (state && typeof state === "object" && "idx" in state && typeof state.idx === "number") return state.idx;
  return 0;
}

/**
 * アプリの中の履歴を戻る・進む。
 * 戻れるかは今の位置（idx）から、進めるかはこれまでに見た最大の idx から求める。
 * 新しい画面に進んだ（PUSH）ときは、その先の履歴が捨てられるので、最大を今の位置に戻す。
 */
export function useHistoryNav(): HistoryNav {
  // 画面が変わるたびに描き直すため、location を購読する
  useLocation();
  const type = useNavigationType();
  const navigate = useNavigate();
  const maxIdx = useRef<number | null>(null);

  const idx = currentIdx();
  // 同じ画面で何度描いても同じ結果になる（PUSH は今の位置、それ以外は最大を保つ）
  maxIdx.current = type === "PUSH" || maxIdx.current === null ? idx : Math.max(maxIdx.current, idx);

  const back = useCallback(() => void navigate(-1), [navigate]);
  const forward = useCallback(() => void navigate(1), [navigate]);

  return { canGoBack: idx > 0, canGoForward: idx < maxIdx.current, back, forward };
}
