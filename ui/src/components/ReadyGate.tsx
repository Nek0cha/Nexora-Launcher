import type { UseQueryResult } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";

/** 待ち始めてからスケルトンを出すまでの猶予。これより早く終わるときは何も出さない */
export const GATE_DELAY_MS = 150;
/** スケルトンを出したら、最低これだけは保つ（ちらつき防止） */
export const GATE_MIN_SHOW_MS = 300;

export type GatePhase = "blank" | "skeleton" | "content";

/**
 * 読み込み中の見せ方を決める。待ちが 150ms を超えたらスケルトン、出したら 300ms は保ち、そのあとに中身。
 * 150ms 未満で終わるときは、スケルトンを挟まず空白から中身へ進む。
 */
export function useGateTiming(pending: boolean): GatePhase {
  const [phase, setPhase] = useState<GatePhase>(pending ? "blank" : "content");
  const shownAt = useRef(0);

  useEffect(() => {
    if (pending) {
      setPhase((p) => (p === "content" ? "blank" : p));
      const id = setTimeout(() => {
        shownAt.current = Date.now();
        setPhase("skeleton");
      }, GATE_DELAY_MS);
      return () => clearTimeout(id);
    }
    if (phase === "skeleton") {
      const rest = Math.max(0, GATE_MIN_SHOW_MS - (Date.now() - shownAt.current));
      const id = setTimeout(() => setPhase("content"), rest);
      return () => clearTimeout(id);
    }
    setPhase("content");
    // phase は意図して依存に入れない（pending の変化だけで進める）
  }, [pending]);

  // 待ちが終わった直後の 1 描画目から空白を挟まない
  if (!pending && phase === "blank") return "content";
  return phase;
}

/**
 * 一度「そろった」になったら、その後に問い合わせが pending に戻っても（取り直しや再試行）、そろったままにする。
 * 中身を出したあとに枠へ戻すと、入力や並びが消えて跳ねる。
 */
export function useLatchedPending(pending: boolean): boolean {
  const passed = useRef(false);
  const latched = !passed.current && pending;
  if (!latched) passed.current = true;
  return latched;
}

/**
 * 最初の問い合わせがすべて終わるまで中身を出さず、同じ配置のスケルトンを見せる。
 * 失敗した問い合わせがあっても枠は外す（失敗の見せ方は子が決める）。
 */
export function ReadyGate({
  queries,
  skeleton,
  children,
}: {
  queries: Pick<UseQueryResult, "isPending">[];
  skeleton: ReactNode;
  children: ReactNode | (() => ReactNode);
}) {
  const phase = useGateTiming(queries.some((q) => q.isPending));
  if (phase === "blank") return null;
  if (phase === "skeleton") return <>{skeleton}</>;
  return <>{typeof children === "function" ? children() : children}</>;
}
