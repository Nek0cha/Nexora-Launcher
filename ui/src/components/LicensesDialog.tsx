import { useEffect, useState } from "react";
import { Button, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCaretRight from "~icons/ph/caret-right";
import { toErrorPayload } from "../api/ipc";
import { FailureNotice } from "./FailureNotice";
import { overlayClass, wideModalClass } from "./modal";

/** 第三者のライセンス。同じ本文は 1 つにまとめてあり、entries の text は texts の番号である */
export type Licenses = {
  entries: { name: string; version: string; license: string; text: number }[];
  texts: string[];
};

/** 本文は大きいので、ダイアログを開いたときに初めて読み込む（別の chunk になる） */
const loadBundled = async (): Promise<Licenses> => (await import("../assets/third-party-licenses.json")).default;

const closeButton = [
  "inline-flex h-9 items-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

const rowButton = [
  "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:bg-surface-3 data-pressed:bg-surface-3",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/**
 * 第三者のライセンスの一覧を見せるダイアログ。全文を一度に描くと数万行になって固まるため、
 * 一覧はパッケージ名だけを並べ、選んだ 1 つの本文だけを開く。
 */
export function LicensesDialog({
  isOpen,
  onOpenChange,
  load = loadBundled,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  load?: () => Promise<Licenses>;
}) {
  const { t } = useTranslation();
  const [data, setData] = useState<Licenses | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [open, setOpen] = useState<number | null>(null);

  useEffect(() => {
    if (!isOpen || data !== null) return;
    let cancelled = false;
    load().then(
      (loaded) => {
        if (!cancelled) {
          setData(loaded);
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
  }, [isOpen, data, load]);

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
            <ul className="scroll-thin flex max-h-[55vh] flex-col overflow-auto">
              {data?.entries.map((entry, i) => {
                const expanded = open === i;
                return (
                  <li key={`${entry.name}@${entry.version}`}>
                    <Button
                      aria-expanded={expanded}
                      onPress={() => setOpen(expanded ? null : i)}
                      className={rowButton}
                    >
                      <IconCaretRight
                        aria-hidden
                        className={`size-3.5 shrink-0 text-text-muted transition-transform duration-(--duration-fast) ${expanded ? "rotate-90" : ""}`}
                      />
                      <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                      <span className="tabular shrink-0 text-text-muted">{entry.version}</span>
                      <span className="shrink-0 text-xs text-text-muted">{entry.license}</span>
                    </Button>
                    {expanded && (
                      <pre className="scroll-thin mx-2 mb-2 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md bg-surface-1 p-3 text-xs text-text-muted">
                        {data.texts[entry.text]}
                      </pre>
                    )}
                  </li>
                );
              })}
            </ul>
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
