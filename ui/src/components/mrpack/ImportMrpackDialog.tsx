import { useQuery } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { Button, Dialog, Heading, Input, Label, Modal, ModalOverlay, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { inspectMrpack } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { MrpackInfo } from "../../bindings/MrpackInfo";
import { useImportMrpack, useMrpackImport } from "../../features/mrpack/useMrpackImport";
import { FailureNotice } from "../FailureNotice";
import { modalClass, overlayClass } from "../modal";
import { Skeleton } from "../Skeleton";
import { controlWidth, inputBase, rowLabel } from "../settingsLayout";
import { onPanel, secondaryButton, submitButton } from "./dialogStyles";

const inputClass = `${inputBase} ${controlWidth}`;

/** 読み込む `.mrpack` を選んだあとの確認。名前を決めて、取り込みを始める。 */
export function ImportMrpackDialog() {
  const { path, close } = useMrpackImport();
  // 閉じる動きの間も中身を残すため、最後に開いたパスを覚えておく
  const shown = useRef<string | null>(null);
  if (path !== null) shown.current = path;
  return (
    <ModalOverlay
      isOpen={path !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      isDismissable
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog className="outline-hidden">
          {shown.current !== null && <ImportForm key={shown.current} path={shown.current} onDone={close} />}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function loaderText(info: MrpackInfo, vanilla: string, loaderName: string): string {
  return info.loader === "vanilla" ? vanilla : `${loaderName} ${info.loaderVersion ?? ""}`.trim();
}

function ImportForm({ path, onDone }: { path: string; onDone: () => void }) {
  const { t } = useTranslation();
  const info = useQuery({
    queryKey: ["mrpack-info", path],
    queryFn: () => inspectMrpack(path),
    retry: false,
    gcTime: 0,
    staleTime: 0,
  });
  const run = useImportMrpack();
  // 利用者が書き換えるまでは、パックの名前を使う
  const [typed, setTyped] = useState<string | null>(null);
  const name = typed ?? info.data?.name ?? "";
  const ready = info.data !== undefined && name.trim() !== "";

  const submit = () => {
    if (!ready || run.isPending) return;
    run.mutate({ path, name: name.trim() }, { onSuccess: onDone });
  };

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <Heading slot="title" className="px-5 pt-5 font-display text-lg font-semibold text-text-strong">
        {t("mrpack.import.title")}
      </Heading>
      <div className="flex flex-col gap-3 px-5 pt-4 pb-5">
        {info.isPending && (
          <div aria-busy="true" className="flex flex-col gap-3">
            {[0, 1, 2, 3].map((i) => (
              <div key={i} className="flex h-9 items-center justify-between gap-4">
                <Skeleton className={`h-5 w-20 ${onPanel}`} />
                <Skeleton className={`h-5 w-28 ${onPanel}`} />
              </div>
            ))}
          </div>
        )}
        {info.error && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(info.error)} />
          </div>
        )}
        {info.data && (
          <>
            <TextField
              value={name}
              onChange={setTyped}
              maxLength={64}
              autoFocus
              className="flex items-center justify-between gap-4"
            >
              <Label className={rowLabel}>{t("create.name")}</Label>
              <Input className={inputClass} />
            </TextField>
            <Row label="Minecraft" value={info.data.mcVersion} />
            <Row
              label={t("create.loader")}
              value={loaderText(info.data, t("create.tab.vanilla"), t(`create.tab.${info.data.loader}`))}
            />
            <Row label={t("mrpack.import.files")} value={String(info.data.fileCount)} />
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
          {t("mrpack.import.submit")}
        </Button>
      </div>
    </form>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex h-9 items-center justify-between gap-4">
      <span className={rowLabel}>{label}</span>
      <span className="tabular min-w-0 truncate text-text">{value}</span>
    </div>
  );
}
