import { useEffect, useState } from "react";
import { Button, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { toErrorPayload } from "../api/ipc";
import { FailureNotice } from "./FailureNotice";
import { overlayClass, wideModalClass } from "./modal";

/** 本文は大きいので、ダイアログを開いたときに初めて読み込む（別の chunk になる） */
const loadBundled = async (): Promise<string> => (await import("../assets/third-party-licenses.txt?raw")).default;

const closeButton = [
  "inline-flex h-9 items-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/** 第三者のライセンスの全文を見せるダイアログ */
export function LicensesDialog({
  isOpen,
  onOpenChange,
  load = loadBundled,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  load?: () => Promise<string>;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    if (!isOpen || text !== null) return;
    let cancelled = false;
    load().then(
      (loaded) => {
        if (!cancelled) {
          setText(loaded);
          setError(null);
        }
      },
      (err) => {
        if (!cancelled) setError(err);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [isOpen, text, load]);

  return (
    <ModalOverlay isOpen={isOpen} onOpenChange={onOpenChange} isDismissable className={overlayClass}>
      <Modal className={wideModalClass}>
        <Dialog className="flex flex-col gap-4 p-5 outline-hidden">
          <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
            {t("licenses.title")}
          </Heading>
          {error ? (
            <div role="alert">
              <FailureNotice error={toErrorPayload(error)} />
            </div>
          ) : (
            <pre className="scroll-thin max-h-[55vh] overflow-auto whitespace-pre-wrap break-words rounded-md bg-surface-1 p-3 text-xs text-text-muted">
              {text}
            </pre>
          )}
          <div className="flex justify-end">
            <Button autoFocus slot="close" className={closeButton}>
              {t("licenses.close")}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
