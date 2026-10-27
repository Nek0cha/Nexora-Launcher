/** 読み込み中の場所取り。大きさは className で渡す。明滅は動きを抑える設定でないときだけ */
export function Skeleton({ className = "" }: { className?: string }) {
  return <div aria-hidden className={`rounded-md bg-surface-2 motion-safe:animate-pulse ${className}`} />;
}
