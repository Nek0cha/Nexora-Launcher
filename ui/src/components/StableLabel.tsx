/**
 * ボタンの文字。切り替わる文字をすべて同じ場所に重ねて置き、いちばん長いものに幅を合わせる。
 * 状態が変わっても幅が変わらず、隣のボタンが動かない。見えない文字は読み上げからも外す。
 */
export function StableLabel(props: { labels: string[]; active: number; className?: string }) {
  return (
    <span className="inline-grid">
      {props.labels.map((label, i) => (
        <span
          // 並びは変わらない（文字の数も位置も固定）
          // biome-ignore lint/suspicious/noArrayIndexKey: 位置で識別する固定の並び
          key={i}
          aria-hidden={i === props.active ? undefined : true}
          className={`col-start-1 row-start-1 whitespace-nowrap ${i === props.active ? "" : "invisible"} ${props.className ?? ""}`}
        >
          {label}
        </span>
      ))}
    </span>
  );
}
