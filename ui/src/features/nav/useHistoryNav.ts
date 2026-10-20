import { useCallback, useMemo, useRef } from "react";
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
  // 反映済みの location を購読する。idx は location.key と組にして、描画のたびに history を読まない
  const location = useLocation();
  const type = useNavigationType();
  const navigate = useNavigate();
  const maxIdx = useRef<{ key: string; max: number } | null>(null);

  const idx = useMemo(() => currentIdx(), [location.key]);
  // 位置（key）が変わったときだけ最大を更新する（PUSH は今の位置、それ以外は最大を保つ）
  if (maxIdx.current === null || maxIdx.current.key !== location.key) {
    const max = type === "PUSH" || maxIdx.current === null ? idx : Math.max(maxIdx.current.max, idx);
    maxIdx.current = { key: location.key, max };
  }
  const max = maxIdx.current.max;

  const back = useCallback(() => void navigate(-1), [navigate]);
  const forward = useCallback(() => void navigate(1), [navigate]);

  return { canGoBack: idx > 0, canGoForward: idx < max, back, forward };
}
