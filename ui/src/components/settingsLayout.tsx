import type { ReactNode } from "react";
import IconInfo from "~icons/ph/info";
import IconWarning from "~icons/ph/warning";

// 右の列の幅。名前の欄と、自動／手動の選択欄で揃える。幅が足りないときは、ラベルでなくこちらが縮む
export const controlWidth = "w-60 min-w-0";

// 左のラベル。縮めず折り返さない（スクロールバーや文字の拡大で幅が減っても、文字の途中で改行しない）
export const rowLabel = "shrink-0 whitespace-nowrap text-text";

export const inputBase = [
  "h-9 rounded-md border border-line-control bg-surface-1 px-3 text-text",
  "placeholder:text-text-faint",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:border-text-muted",
  "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
].join(" ");

// 1段の行は 44px、ラベルと説明の2段の行は 52px（アプリの設定の画面と同じ）
export function Row({ children }: { children: ReactNode }) {
  return <div className="flex min-h-11 flex-col justify-center gap-2 py-2">{children}</div>;
}

export function Notice({ tone, children }: { tone: "info" | "warning"; children: ReactNode }) {
  const Icon = tone === "warning" ? IconWarning : IconInfo;
  return (
    <p className="flex items-start gap-2 text-sm text-text">
      <Icon aria-hidden className={`mt-0.5 size-4 shrink-0 ${tone === "warning" ? "text-warning" : "text-text-muted"}`} />
      <span>{children}</span>
    </p>
  );
}
