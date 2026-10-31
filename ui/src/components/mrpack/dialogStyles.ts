// 読み込みと書き出しのダイアログで共有する見た目（作成のダイアログと同じ形）
export const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

export const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3",
  ring,
].join(" ");

export const submitButton = [
  "inline-flex h-9 items-center gap-2 rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

// 見出しの横や行の中に置く控えめな操作。面は乗せたときだけ現れる
export const quietTrigger = [
  "inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

// ダイアログの面（surface-2）の上では、ふだんのスケルトンの色が溶けるため、一段濃くする
export const onPanel = "bg-surface-3!";
