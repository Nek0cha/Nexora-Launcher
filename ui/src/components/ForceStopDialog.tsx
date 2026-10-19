import { useId } from "react";
import { Button, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useStopFlow } from "../features/play/useStopFlow";
import { centeredOverlayClass, modalClass } from "./modal";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3",
  ring,
].join(" ");

const dangerButton = [
  "inline-flex h-9 items-center rounded-md bg-danger px-4 text-sm font-medium text-danger-ink",
  "transition-opacity duration-(--duration-fast) ease-out-quint data-hovered:opacity-90",
  ring,
].join(" ");

/**
 * 「閉じて」と頼んでから 10 秒たってもゲームが終了しないときの確認（spec 3.2）。
 * 標準の操作（最初のフォーカス）は「待つ」。何も選ばずに閉じたときも待ち続ける。
 * ゲームが終了すると、useStopFlow の状態が消えて、このダイアログも閉じる。アプリに 1 つだけ置く。
 */
export function ForceStopDialog() {
  const { t } = useTranslation();
  const flow = useStopFlow();
  const bodyId = useId();
  const instanceId = flow.confirmingFor;

  return (
    <ModalOverlay
      isOpen={instanceId !== null}
      onOpenChange={(open) => {
        if (!open) flow.keepWaiting();
      }}
      isDismissable
      className={centeredOverlayClass}
    >
      <Modal className={`${modalClass} max-w-md`}>
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-4 p-5 outline-hidden">
          <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
            {t("forceStop.title")}
          </Heading>
          <p id={bodyId} className="text-sm text-text">
            {t("forceStop.body")}
          </p>
          <div className="flex justify-end gap-2 pt-1">
            <Button autoFocus onPress={flow.keepWaiting} className={secondaryButton}>
              {t("forceStop.wait")}
            </Button>
            <Button onPress={() => instanceId && flow.forceStop(instanceId)} className={dangerButton}>
              {t("forceStop.confirm")}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
