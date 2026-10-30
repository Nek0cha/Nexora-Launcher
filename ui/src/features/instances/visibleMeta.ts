import { useLayoutEffect, useState } from "react";

/** 一覧の行の名前の下に並べる情報。並べる順 */
export type MetaKey = "version" | "lastPlayed" | "playTime";

const ORDER: MetaKey[] = ["version", "lastPlayed", "playTime"];

/**
 * 各情報を出すのに要る、行の幅（px）の下限。状態の印は名前の横に出すので、ここには含めない。
 * 隠す順は、プレイ時間、最終プレイ、バージョン。下限が大きいものから先に消える。
 * 値は、通常のバージョン（1.21.5）で、そこまでの情報が 1 行に収まる幅（ja と en の長い方、実測）。
 * 長いスナップショットのバージョンは、行の側で切り詰める。
 */
export const META_MIN_WIDTH: Record<MetaKey, number> = {
  version: 56,
  lastPlayed: 272,
  playTime: 406,
};

/** 行の名前の列の幅で、いま出す情報を決める。実際の幅を知っているのはブラウザだけなので、幅は引数で受ける */
export function visibleMeta(width: number): MetaKey[] {
  return ORDER.filter((key) => width >= META_MIN_WIDTH[key]);
}

/** 要素の幅を追い、`visibleMeta` の結果を返す。幅が分からない間（まだ測れていない、画面が無い）は全部出す */
export function useVisibleMeta(ref: { current: HTMLElement | null }): MetaKey[] {
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = () => setWidth(el.clientWidth > 0 ? el.clientWidth : null);
    read();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(read);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return visibleMeta(width ?? Number.POSITIVE_INFINITY);
}
