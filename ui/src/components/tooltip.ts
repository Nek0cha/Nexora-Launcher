// ツールチップの見た目。タスクの一覧とインスタンスの詳細のアイコンのボタンで共有する
export const tooltipClass = [
  "rounded-sm border border-line bg-surface-2 px-2 py-1 text-xs text-text shadow-float",
  "transition-[opacity,translate] duration-(--duration-fast) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0",
  "motion-safe:data-[placement=top]:data-entering:translate-y-1 motion-safe:data-[placement=bottom]:data-entering:-translate-y-1",
  "motion-reduce:transition-opacity",
].join(" ");
