import type { PointerEvent } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconX from "~icons/ph/x-bold";

const removeButton = [
  "-mr-1 inline-flex size-5 shrink-0 items-center justify-center rounded-full text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 hover:text-text pressed:bg-surface-3 data-disabled:opacity-50",
  "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent",
].join(" ");

/**
 * インスタンスの名前のチップ。つかんでドラッグでき（onDragStart）、onRemove があれば「×」で外せる。
 * ドラッグはマウスとタッチのための近道で、同じ操作は「×」と「インスタンスを追加」のボタンでもできる。
 */
export function InstanceChip({ name, onRemove, removeDisabled, onDragStart, dimmed }: {
  name: string;
  onRemove?: () => void;
  removeDisabled?: boolean;
  onDragStart?: (event: PointerEvent) => void;
  /** ドラッグ中の元の位置。動かしている最中であることを薄くして示す */
  dimmed?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <span
      onPointerDown={(e) => {
        if (!onDragStart || e.button !== 0) return;
        // 「×」を押したときはドラッグを始めない
        if ((e.target as Element).closest("button")) return;
        // 文字の選択を始めない
        e.preventDefault();
        onDragStart(e);
      }}
      className={[
        "inline-flex h-7 max-w-full items-center gap-1.5 rounded-full border border-line bg-surface-1 pr-2 pl-2.5 text-sm text-text select-none",
        onDragStart ? "cursor-grab touch-none" : "",
        dimmed ? "opacity-40" : "",
      ].join(" ")}
    >
      <span className="min-w-0 truncate">{name}</span>
      {onRemove && (
        <Button
          aria-label={t("sync.screen.remove", { name })}
          onPress={onRemove}
          isDisabled={removeDisabled}
          className={removeButton}
        >
          <IconX aria-hidden className="size-3" />
        </Button>
      )}
    </span>
  );
}
