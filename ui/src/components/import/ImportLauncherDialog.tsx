import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button, Checkbox, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconWarning from "~icons/ph/warning";
import { findImportDuplicates, listImportInstances } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import { toErrorPayload } from "../../api/ipc";
import type { ImportKind } from "../../bindings/ImportKind";
import type { SourceInstance } from "../../bindings/SourceInstance";
import {
  closeLauncherImport,
  useImportFromLauncher,
  useLauncherImportOpen,
} from "../../features/import/useLauncherImport";
import { FailureNotice } from "../FailureNotice";
import { modalClass, overlayClass } from "../modal";
import { Skeleton } from "../Skeleton";
import { checkboxClass, checkboxMark, onPanel, secondaryButton, submitButton } from "../mrpack/dialogStyles";

const SOURCE = "modrinth_app" as const;

/** 他のランチャーのインスタンスを選び、インスタンスごとに取り込む中身を選ぶ。 */
export function ImportLauncherDialog() {
  const open = useLauncherImportOpen();
  return (
    <ModalOverlay
      isOpen={open}
      onOpenChange={(next) => {
        if (!next) closeLauncherImport();
      }}
      isDismissable
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog className="outline-hidden">{open && <ImportForm />}</Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function Mark() {
  return (
    <span aria-hidden className={checkboxMark}>
      <IconCheck className="size-3 opacity-0 group-data-selected:opacity-100" />
    </span>
  );
}

function ImportForm() {
  const { t } = useTranslation();
  const [root, setRoot] = useState<string | null>(null);
  // 選んだ行と、その行で取り込む種類
  const [picked, setPicked] = useState<Record<string, ImportKind[]>>({});
  const [duplicates, setDuplicates] = useState<string[]>([]);
  const list = useQuery({
    queryKey: ["import-instances", SOURCE, root],
    queryFn: () => listImportInstances(SOURCE, root),
    retry: false,
    gcTime: 0,
    staleTime: 0,
  });
  const run = useImportFromLauncher();

  const ids = Object.keys(picked);
  const count = ids.length;
  const anyway = duplicates.length > 0;

  const change = (next: Record<string, ImportKind[]>) => {
    setPicked(next);
    setDuplicates([]);
  };
  const toggleRow = (row: SourceInstance, on: boolean) => {
    if (on) change({ ...picked, [row.id]: [...row.available] });
    else {
      const { [row.id]: _removed, ...rest } = picked;
      change(rest);
    }
  };
  const toggleKind = (id: string, kind: ImportKind, on: boolean) => {
    const kinds = picked[id] ?? [];
    change({ ...picked, [id]: on ? [...kinds, kind] : kinds.filter((k) => k !== kind) });
  };

  const submit = useMutation({
    mutationFn: async () => {
      if (!anyway) {
        const found = await findImportDuplicates(SOURCE, root, ids);
        if (found.length > 0) {
          setDuplicates(found);
          return false;
        }
      }
      const requests = ids.map((id) => ({ id, kinds: picked[id] ?? [] }));
      await run.mutateAsync({ requests, allowDuplicates: anyway, root });
      return true;
    },
    onSuccess: (started) => {
      if (started) closeLauncherImport();
    },
  });

  const choose = () => {
    void pickFolder().then((dir) => {
      if (dir !== null) {
        setRoot(dir);
        setPicked({});
      }
    });
  };

  const listError = list.error ? toErrorPayload(list.error) : null;
  const rows = list.data ?? [];

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (count > 0 && !submit.isPending) submit.mutate();
      }}
    >
      <Heading slot="title" className="px-5 pt-5 font-display text-lg font-semibold text-text-strong">
        {t("launcherImport.title")}
      </Heading>
      <div className="flex flex-col gap-3 px-5 pt-4 pb-5">
        {list.isPending && (
          <div aria-busy="true" className="flex flex-col gap-3">
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} className={`h-9 w-full ${onPanel}`} />
            ))}
          </div>
        )}
        {listError && (
          <div role="alert" className="flex items-center justify-between gap-3">
            {listError.code === "import.unavailable" ? (
              <Button type="button" onPress={choose} className={secondaryButton}>
                {t("launcherImport.chooseFolder")}
              </Button>
            ) : (
              <FailureNotice error={listError} />
            )}
          </div>
        )}
        {list.data && rows.length === 0 && <p className="text-text-muted">{t("launcherImport.empty")}</p>}
        {rows.length > 0 && (
          <ul className="scroll-thin flex max-h-96 flex-col gap-2 overflow-y-auto">
            {rows.map((row) => {
              const selected = row.id in picked;
              const duplicated = duplicates.includes(row.id);
              return (
                <li key={row.id} className="flex flex-col gap-2 rounded-md border border-line bg-surface-1 p-3">
                  <div className="flex items-center gap-3">
                    <Checkbox
                      isSelected={selected}
                      isDisabled={row.unsupported !== null}
                      onChange={(on) => toggleRow(row, on)}
                      className={`${checkboxClass} min-w-0 data-disabled:opacity-50`}
                    >
                      <Mark />
                      <span className="truncate">{row.name}</span>
                    </Checkbox>
                    <span className="tabular shrink-0 text-sm text-text-muted">
                      {row.mcVersion} · {t(`create.tab.${row.loader}`)}
                    </span>
                    {duplicated && (
                      <span className="ml-auto flex shrink-0 items-center gap-1 text-sm text-warning">
                        <IconWarning aria-hidden className="size-4" />
                        {t("launcherImport.duplicate")}
                      </span>
                    )}
                  </div>
                  {row.unsupported !== null && <p className="pl-6 text-sm text-text-muted">{row.unsupported}</p>}
                  {selected && (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 pl-6">
                      {row.available.map((kind) => (
                        <Checkbox
                          key={kind}
                          isSelected={picked[row.id]?.includes(kind) ?? false}
                          onChange={(on) => toggleKind(row.id, kind, on)}
                          className={`${checkboxClass} text-sm`}
                        >
                          <Mark />
                          {t(`launcherImport.kind.${kind}`)}
                        </Checkbox>
                      ))}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
        {submit.error && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(submit.error)} />
          </div>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-5 py-3">
        <Button slot="close" className={secondaryButton}>
          {t("create.cancel")}
        </Button>
        <Button type="submit" isDisabled={count === 0} isPending={submit.isPending} className={submitButton}>
          {anyway
            ? t("launcherImport.continue")
            : count === 0
              ? t("launcherImport.submit")
              : t("launcherImport.submitCount", { count })}
        </Button>
      </div>
    </form>
  );
}
