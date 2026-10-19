import { useId } from "react";
import { Button, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { modalClass, overlayClass } from "./modal";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

const dangerButton = [
  "inline-flex h-9 items-center rounded-md bg-danger px-4 text-sm font-medium text-danger-ink",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

/**
 * インスタンスの削除の確認（spec 3.1）。名前と、ワールドを含むすべてのデータが消えて元に戻せないことを伝える。
 * 標準の操作（最初のフォーカス）は「キャンセル」である。
 *
 * 開閉は DialogTrigger の中に置けばそれに従う。メニューから開くときのように引き金のボタンがない場合は、
 * isOpen と onOpenChange で外から開閉する。
 */
export function ConfirmDeleteDialog({
  name,
  onConfirm,
  isPending,
  isOpen,
  onOpenChange,
}: {
  name: string;
  onConfirm: () => void;
  isPending: boolean;
  isOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const bodyId = useId();
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={(open) => {
        // 削除の最中は閉じない（結果を待つ）
        if (!open && isPending) return;
        onOpenChange?.(open);
      }}
      isDismissable={!isPending}
      isKeyboardDismissDisabled={isPending}
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-4 p-5 outline-hidden">
          <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
            {t("instances.deleteTitle", { name })}
          </Heading>
          <p id={bodyId} className="text-sm text-text">
            {t("instances.deleteBody")}
          </p>
          <div className="flex justify-end gap-2 pt-1">
            <Button autoFocus slot="close" isDisabled={isPending} className={secondaryButton}>
              {t("instances.cancel")}
            </Button>
            <Button onPress={onConfirm} isPending={isPending} className={dangerButton}>
              {t("instances.deleteConfirm")}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
