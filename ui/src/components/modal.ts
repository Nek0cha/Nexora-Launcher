// モーダルの暗幕と出入りの動き。コマンドパレットと作成のダイアログで共有する

// 背景の暗幕は不透明度だけを変える
export const overlayClass = [
  "fixed inset-0 z-50 flex items-start justify-center bg-surface-0/60 p-4 pt-[15vh]",
  "transition-opacity duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0 data-exiting:duration-(--duration-fast)",
].join(" ");

// 下から 8px 動きながら現れる。閉じるときは fast。動きを減らす設定では不透明度だけ
export const modalClass = [
  "w-full max-w-lg rounded-lg border border-line bg-surface-2 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 motion-safe:data-entering:translate-y-2",
  "data-exiting:opacity-0 motion-safe:data-exiting:translate-y-2 data-exiting:duration-(--duration-fast)",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");
