import { type ReactNode, useId } from "react";
import { Button, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { modalClass, overlayClass } from "./modal";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

const confirmButton = {
  danger: "bg-danger text-danger-ink",
  default: "bg-text-strong text-surface-0",
} as const;

const confirmBase = [
  "inline-flex h-9 items-center rounded-md px-4 text-sm font-medium",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

/**
 * 確認のダイアログ（インスタンスの削除の確認と同じ形）。標準の操作（最初のフォーカス）は「キャンセル」である。
 * 開閉は isOpen と onOpenChange で外から行う。実行の最中（isPending）は閉じない。
 */
export function ConfirmDialog(props: {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  tone: "danger" | "default";
  onConfirm: () => void;
  isPending: boolean;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const bodyId = useId();
  const { isPending } = props;
  return (
    <ModalOverlay
      isOpen={props.isOpen}
      onOpenChange={(open) => {
        if (!open && isPending) return;
        props.onOpenChange(open);
      }}
      isDismissable={!isPending}
      isKeyboardDismissDisabled={isPending}
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-4 p-5 outline-hidden">
          <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
            {props.title}
          </Heading>
          <div id={bodyId} className="flex flex-col gap-3 text-sm text-text">
            {props.children}
          </div>
          <div className="flex justify-end gap-2 pt-1">
            <Button autoFocus slot="close" isDisabled={isPending} className={secondaryButton}>
              {props.cancelLabel}
            </Button>
            <Button
              onPress={props.onConfirm}
              isPending={isPending}
              className={`${confirmBase} ${confirmButton[props.tone]}`}
            >
              {props.confirmLabel}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
