import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconNext from "~icons/ph/arrow-right";
import IconPrevious from "~icons/ph/arrow-left";
import IconDrop from "~icons/ph/download-simple";
import IconScreenshots from "~icons/ph/image";
import IconDelete from "~icons/ph/trash";
import IconClose from "~icons/ph/x";
import { fileUrl } from "../api/assets";
import { onFilesDropped } from "../api/dragDrop";
import { toErrorPayload } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { Screenshot } from "../bindings/Screenshot";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";
import { centeredOverlayClass } from "../components/modal";
import { useToast } from "../components/toast/useToast";
import { useScreenshotActions, useScreenshots } from "../features/screenshots/useScreenshots";
import i18n from "../i18n";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const iconButton = [
  "inline-flex size-9 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 hover:text-text pressed:bg-surface-3 data-disabled:opacity-40",
  ring,
].join(" ");

const thumbButton = [
  "group relative block aspect-video w-full overflow-hidden rounded-md border border-line bg-surface-1",
  "transition-colors duration-(--duration-fast) ease-out-quint hover:border-line-control",
  ring,
].join(" ");

/** 閉じる、削除など、ビューアの操作に使う名前（ファイル名から拡張子を除く） */
const label = (name: string) => name.replace(/\.[^.]*$/, "") || name;

/**
 * インスタンスのスクリーンショット。新しい順のサムネイルの一覧で、押すと大きく開く。
 * 画像のファイルをウィンドウに落とすと、このインスタンスのフォルダへ写す。
 */
export function ScreenshotsTab({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const toast = useToast();
  const { data, error } = useScreenshots(instance.id);
  const { remove, add } = useScreenshotActions(instance.id);
  // 開いている画像は、削除で一覧が変わっても追えるよう、位置ではなく名前で持つ
  const [viewing, setViewing] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [dragging, setDragging] = useState(false);
  const date = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }),
    // 言語を替えたときに作り直す
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [i18n.language],
  );

  const shots = data ?? [];
  const index = viewing === null ? -1 : shots.findIndex((s) => s.name === viewing);
  const current: Screenshot | undefined = shots[index];
  // 開いている画像が一覧から消えた（外から削除されたなど）ときは閉じる
  useEffect(() => {
    if (viewing !== null && data && index === -1) {
      setViewing(null);
      setConfirming(false);
    }
  }, [viewing, data, index]);

  // ドロップの受け口は 1 度だけ登録するので、最新の値は ref 越しに読む
  const blocked = viewing !== null || add.isPending;
  const latest = useRef({ blocked, add, toast, t });
  latest.current = { blocked, add, toast, t };

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    onFilesDropped({
      enter: () => {
        if (!latest.current.blocked) setDragging(true);
      },
      leave: () => setDragging(false),
      drop: (paths) => {
        setDragging(false);
        const { blocked: busy, add: adding, toast: toaster, t: tr } = latest.current;
        if (busy || paths.length === 0) return;
        adding.mutate(
          { paths },
          {
            onSuccess: ({ added, skipped }) => {
              if (added === 0) toaster.show({ kind: "error", message: tr("screenshots.importNone") });
              else if (skipped === 0) toaster.show({ kind: "success", message: tr("screenshots.imported", { count: added }) });
              else toaster.show({ kind: "info", message: tr("screenshots.importedSome", { added, skipped }) });
            },
            onError: toaster.showError,
          },
        );
      },
    })
      .then((fn) => {
        // 登録が終わる前に閉じたときは、ここで解除する
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const step = (delta: number) => {
    const next = shots[index + delta];
    if (next) setViewing(next.name);
  };

  const confirmDelete = () => {
    if (!current) return;
    // 消したあとは、次の画像（なければ前の画像）へ移る
    const after = shots[index + 1] ?? shots[index - 1];
    remove.mutate(
      { name: current.name },
      {
        onSuccess: () => {
          setConfirming(false);
          setViewing(after?.name ?? null);
        },
        onError: (e) => {
          setConfirming(false);
          toast.showError(e);
        },
      },
    );
  };

  if (!data && error)
    return (
      <div role="alert">
        <FailureNotice error={toErrorPayload(error)} />
      </div>
    );
  if (!data) return null;

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {shots.length === 0 ? (
        <EmptyState icon={IconScreenshots} title={t("screenshots.empty")} />
      ) : (
        <ul
          aria-label={t("screenshots.gallery")}
          className="grid min-h-0 flex-1 grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] content-start gap-3 overflow-y-auto pr-1"
        >
          {shots.map((shot) => (
            <li key={shot.name}>
              <Button
                aria-label={t("screenshots.open", { name: label(shot.name) })}
                onPress={() => setViewing(shot.name)}
                className={thumbButton}
              >
                <img
                  src={fileUrl(shot.path)}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  className="size-full object-cover"
                />
                <span className="tabular absolute inset-x-0 bottom-0 bg-surface-0/70 px-2 py-1 text-left text-xs text-text opacity-0 transition-opacity duration-(--duration-fast) group-hover:opacity-100 group-focus-visible:opacity-100">
                  {date.format(shot.modifiedMs)}
                </span>
              </Button>
            </li>
          ))}
        </ul>
      )}
      {dragging && (
        // 受け皿。ドラッグの間だけ、タブ全体を覆って出す
        <div className="pointer-events-none absolute -inset-3 z-10 flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-accent bg-surface-0/85 backdrop-blur-sm">
          <IconDrop aria-hidden className="size-8 text-accent" />
          <p className="font-display text-lg font-semibold text-text-strong">{t("screenshots.dropTarget")}</p>
          <p className="text-sm text-text-muted">{t("screenshots.dropAccepts")}</p>
        </div>
      )}
      <ModalOverlay
        isOpen={current !== undefined && !confirming}
        onOpenChange={(open) => {
          if (!open) setViewing(null);
        }}
        isDismissable
        className={centeredOverlayClass}
      >
        <Modal className="flex max-h-full w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-line bg-surface-2 shadow-float">
          <Dialog aria-label={t("screenshots.viewer")} className="flex min-h-0 flex-col outline-hidden">
            {current && (
              // 矢印キーで前後の画像へ
              <div
                className="flex min-h-0 flex-col"
                onKeyDown={(e) => {
                  if (e.key === "ArrowLeft") step(-1);
                  else if (e.key === "ArrowRight") step(1);
                }}
              >
                <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-2">
                  <Heading slot="title" className="tabular min-w-0 truncate text-sm text-text">
                    {date.format(current.modifiedMs)}
                  </Heading>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      aria-label={t("screenshots.previous")}
                      isDisabled={index <= 0}
                      onPress={() => step(-1)}
                      className={iconButton}
                    >
                      <IconPrevious aria-hidden className="size-4" />
                    </Button>
                    <Button
                      aria-label={t("screenshots.next")}
                      isDisabled={index >= shots.length - 1}
                      onPress={() => step(1)}
                      className={iconButton}
                    >
                      <IconNext aria-hidden className="size-4" />
                    </Button>
                    <Button aria-label={t("screenshots.delete")} onPress={() => setConfirming(true)} className={iconButton}>
                      <IconDelete aria-hidden className="size-4" />
                    </Button>
                    <Button aria-label={t("screenshots.close")} slot="close" className={iconButton}>
                      <IconClose aria-hidden className="size-4" />
                    </Button>
                  </div>
                </div>
                <div className="flex min-h-0 flex-1 items-center justify-center bg-surface-0 p-2">
                  <img src={fileUrl(current.path)} alt={label(current.name)} className="max-h-[75vh] max-w-full object-contain" />
                </div>
              </div>
            )}
          </Dialog>
        </Modal>
      </ModalOverlay>
      <ConfirmDialog
        title={t("screenshots.deleteTitle", { name: current ? label(current.name) : "" })}
        confirmLabel={t("screenshots.delete")}
        cancelLabel={t("instances.cancel")}
        tone="danger"
        isOpen={confirming && current !== undefined}
        onOpenChange={setConfirming}
        onConfirm={confirmDelete}
        isPending={remove.isPending}
      >
        <p>{t("screenshots.deleteBody")}</p>
      </ConfirmDialog>
    </div>
  );
}
