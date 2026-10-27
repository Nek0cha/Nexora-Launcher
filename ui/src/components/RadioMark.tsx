/**
 * 選択肢の丸い印。親の group の選択の状態に応じて点の色と大きさ（scale）だけが変わる。
 * 枠の太さは変えず、点は選択されていなくても常に描く。
 */
export function RadioMark(props: { className?: string }) {
  return (
    <span
      aria-hidden
      className={[
        "grid size-4 shrink-0 place-items-center rounded-full border border-line-control bg-surface-1",
        "transition-colors duration-(--duration-fast) ease-out-quint",
        "group-data-selected:border-accent",
        "group-data-hovered:border-text-muted group-data-selected:group-data-hovered:border-accent",
        "group-data-disabled:border-line group-data-disabled:bg-surface-2",
        props.className ?? "",
      ].join(" ")}
    >
      <span
        data-radio-dot
        aria-hidden
        className={[
          "size-2 scale-0 rounded-full bg-accent",
          "transition-transform duration-(--duration-fast) ease-out-quint",
          "group-data-selected:scale-100 motion-reduce:transition-none",
        ].join(" ")}
      />
    </span>
  );
}
