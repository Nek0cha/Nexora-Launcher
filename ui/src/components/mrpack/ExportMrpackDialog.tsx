import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import {
  Button,
  Checkbox,
  Dialog,
  Heading,
  Input,
  Label,
  Modal,
  ModalOverlay,
  TextField,
  Tooltip,
  TooltipTrigger,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconExport from "~icons/ph/export";
import IconWarning from "~icons/ph/warning";
import { exportMrpack, previewMrpackExport } from "../../api/commands";
import { pickMrpackSavePath } from "../../api/dialog";
import { toErrorPayload } from "../../api/ipc";
import { FailureNotice } from "../FailureNotice";
import { modalClass, overlayClass } from "../modal";
import { Skeleton } from "../Skeleton";
import { controlWidth, inputBase, rowLabel } from "../settingsLayout";
import { useToast } from "../toast/useToast";
import { tooltipClass } from "../tooltip";
import { onPanel, ring, secondaryButton, submitButton } from "./dialogStyles";

const iconButton = [
  "inline-flex size-8 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

const inputClass = `${inputBase} ${controlWidth}`;

const checkboxClass = `group flex w-fit items-center gap-2 text-text ${ring}`;
const checkboxMark =
  "flex size-4 items-center justify-center rounded-sm border border-line-control bg-surface-1 text-surface-0 group-data-selected:border-text-strong group-data-selected:bg-text-strong";

/** 保存ダイアログの既定のファイル名。ファイル名に使えない文字は `_` にする。 */
function defaultFileName(instanceName: string): string {
  const safe = instanceName.replace(/[\\/:*?"<>|]/g, "_").trim();
  return `${safe === "" ? "pack" : safe}.mrpack`;
}

/** インスタンスの詳細の見出しに置く、`.mrpack` として書き出す操作（アイコンのボタンと確認のダイアログ）。 */
export function ExportMrpackDialog({ instanceId, instanceName }: { instanceId: string; instanceName: string }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <>
      <TooltipTrigger delay={500}>
        <Button aria-label={t("mrpack.export.open")} onPress={() => setOpen(true)} className={iconButton}>
          <IconExport aria-hidden className="size-4" />
        </Button>
        <Tooltip offset={6} className={tooltipClass}>
          {t("mrpack.export.open")}
        </Tooltip>
      </TooltipTrigger>
      <ModalOverlay isOpen={open} onOpenChange={setOpen} isDismissable className={overlayClass}>
        <Modal className={modalClass}>
          <Dialog className="outline-hidden">
            <ExportForm instanceId={instanceId} instanceName={instanceName} onDone={() => setOpen(false)} />
          </Dialog>
        </Modal>
      </ModalOverlay>
    </>
  );
}

function ExportForm({
  instanceId,
  instanceName,
  onDone,
}: {
  instanceId: string;
  instanceName: string;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  // 開くたびに作り直されるので、そのつど内容を調べ直す
  const preview = useQuery({
    queryKey: ["mrpack-export-preview", instanceId],
    queryFn: () => previewMrpackExport(instanceId),
    retry: false,
    gcTime: 0,
    staleTime: 0,
  });
  const [versionId, setVersionId] = useState("1.0.0");
  const [includeConfig, setIncludeConfig] = useState(true);
  const [confirmed, setConfirmed] = useState(false);

  const unidentified = preview.data?.unidentified ?? [];
  const needsConfirmation = unidentified.length > 0;
  const ready = preview.data !== undefined && versionId.trim() !== "" && (!needsConfirmation || confirmed);

  const run = useMutation({
    mutationFn: async () => {
      const dest = await pickMrpackSavePath(defaultFileName(instanceName));
      if (dest === null) return false;
      await exportMrpack(instanceId, dest, {
        versionId: versionId.trim(),
        includeConfig,
        acceptUnidentified: needsConfirmation && confirmed,
      });
      return true;
    },
    onSuccess: (exported) => {
      if (!exported) return;
      toast.show({ kind: "success", message: t("mrpack.export.done") });
      onDone();
    },
  });

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (ready && !run.isPending) run.mutate();
      }}
    >
      <Heading slot="title" className="px-5 pt-5 font-display text-lg font-semibold text-text-strong">
        {t("mrpack.export.title")}
      </Heading>
      <div className="flex flex-col gap-3 px-5 pt-4 pb-5">
        {preview.isPending && (
          <div aria-busy="true" className="flex flex-col gap-3">
            <span role="status" className="sr-only">
              {t("mrpack.export.loading")}
            </span>
            {[0, 1].map((i) => (
              <div key={i} className="flex h-9 items-center justify-between gap-4">
                <Skeleton className={`h-5 w-20 ${onPanel}`} />
                <Skeleton className={`h-9 ${controlWidth} ${onPanel}`} />
              </div>
            ))}
          </div>
        )}
        {preview.error && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(preview.error)} />
          </div>
        )}
        {preview.data && (
          <>
            <TextField
              value={versionId}
              onChange={setVersionId}
              maxLength={32}
              autoFocus
              className="flex items-center justify-between gap-4"
            >
              <Label className={rowLabel}>{t("mrpack.export.version")}</Label>
              <Input className={inputClass} />
            </TextField>
            <Checkbox isSelected={includeConfig} onChange={setIncludeConfig} className={checkboxClass}>
              <span aria-hidden className={checkboxMark}>
                <IconCheck className="size-3 opacity-0 group-data-selected:opacity-100" />
              </span>
              {t("mrpack.export.includeConfig")}
            </Checkbox>
            {needsConfirmation && (
              <div className="flex flex-col gap-2 rounded-md border border-line bg-surface-1 p-3">
                <p className="flex items-center gap-2 text-sm text-text">
                  <IconWarning aria-hidden className="size-4 shrink-0 text-warning" />
                  <span>{t("mrpack.export.warning")}</span>
                </p>
                <p className="text-sm text-text-muted">{t("mrpack.export.unidentified")}</p>
                <ul className="scroll-thin flex max-h-40 flex-col gap-0.5 overflow-y-auto text-sm text-text">
                  {unidentified.map((name) => (
                    <li key={name} className="truncate">
                      {name}
                    </li>
                  ))}
                </ul>
                <Checkbox isSelected={confirmed} onChange={setConfirmed} className={checkboxClass}>
                  <span aria-hidden className={checkboxMark}>
                    <IconCheck className="size-3 opacity-0 group-data-selected:opacity-100" />
                  </span>
                  {t("mrpack.export.confirm")}
                </Checkbox>
              </div>
            )}
          </>
        )}
        {run.error && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(run.error)} />
          </div>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-5 py-3">
        <Button slot="close" className={secondaryButton}>
          {t("create.cancel")}
        </Button>
        <Button type="submit" isDisabled={!ready} isPending={run.isPending} className={submitButton}>
          {t("mrpack.export.submit")}
        </Button>
      </div>
    </form>
  );
}
