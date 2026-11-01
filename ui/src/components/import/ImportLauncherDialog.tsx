import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button, Checkbox, Dialog, Heading, Modal, ModalOverlay, Radio, RadioGroup } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconFolder from "~icons/ph/folder-open";
import IconWarning from "~icons/ph/warning";
import { findImportDuplicates, listImportInstances } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import { toErrorPayload } from "../../api/ipc";
import type { ImportKind } from "../../bindings/ImportKind";
import type { ImportSourceKind } from "../../bindings/ImportSourceKind";
import type { SourceInstance } from "../../bindings/SourceInstance";
import {
  closeLauncherImport,
  useImportFromLauncher,
  useLauncherImportOpen,
} from "../../features/import/useLauncherImport";
import { FailureNotice } from "../FailureNotice";
import { modalClass, overlayClass } from "../modal";
import { Skeleton } from "../Skeleton";
import { checkboxClass, checkboxMark, onPanel, ring, secondaryButton, submitButton } from "../mrpack/dialogStyles";

const SOURCES: ImportSourceKind[] = ["modrinth_app", "prism_launcher", "official_launcher", "curse_forge"];

// ランチャーのタブが、ふつうの窓ではスクロールも折り返しもなく並ぶ幅にする
const launcherModalClass = modalClass.replace("max-w-lg", "max-w-3xl");

const sourceClass = [
  "inline-flex h-8 shrink-0 cursor-default items-center gap-1.5 whitespace-nowrap rounded-md px-3 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:bg-surface-2 data-hovered:text-text data-selected:bg-surface-3 data-selected:text-text-strong",
  ring,
].join(" ");

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
      <Modal className={launcherModalClass}>
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
  const [source, setSource] = useState<ImportSourceKind>("modrinth_app");
  const [root, setRoot] = useState<string | null>(null);
  // 選んだ行と、その行で取り込む種類
  const [picked, setPicked] = useState<ReadonlyMap<string, ImportKind[]>>(new Map());
  const [duplicates, setDuplicates] = useState<string[]>([]);
  const list = useQuery({
    queryKey: ["import-instances", source, root],
    queryFn: () => listImportInstances(source, root),
    retry: false,
    gcTime: 0,
    staleTime: 0,
  });
  const run = useImportFromLauncher();

  const ids = [...picked.keys()];
  const count = ids.length;
  const anyway = duplicates.length > 0;

  const change = (next: ReadonlyMap<string, ImportKind[]>) => {
    setPicked(next);
    setDuplicates([]);
  };
  const toggleRow = (row: SourceInstance, on: boolean) => {
    const next = new Map(picked);
    if (on) next.set(row.id, [...row.available]);
    else next.delete(row.id);
    change(next);
  };
  const toggleKind = (id: string, kind: ImportKind, on: boolean) => {
    const kinds = picked.get(id) ?? [];
    change(new Map(picked).set(id, on ? [...kinds, kind] : kinds.filter((k) => k !== kind)));
  };

  const submit = useMutation({
    mutationFn: async () => {
      if (!anyway) {
        const found = await findImportDuplicates(source, root, ids);
        if (found.length > 0) {
          setDuplicates(found);
          return false;
        }
      }
      const requests = ids.map((id) => ({ id, kinds: picked.get(id) ?? [] }));
      await run.mutateAsync({ source, requests, allowDuplicates: anyway, root });
      return true;
    },
    onSuccess: (started) => {
      if (started) closeLauncherImport();
    },
  });

  const chooseSource = (next: ImportSourceKind) => {
    setSource(next);
    setRoot(null);
    change(new Map());
    submit.reset();
  };

  const choose = () => {
    void pickFolder().then((dir) => {
      if (dir !== null) {
        setRoot(dir);
        change(new Map());
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
        <div className="flex items-start justify-between gap-2">
          <RadioGroup
            aria-label={t("launcherImport.sourceLabel")}
            orientation="horizontal"
            value={source}
            onChange={(value) => chooseSource(value as ImportSourceKind)}
            className="flex flex-wrap gap-1"
          >
            {SOURCES.map((id) => (
              <Radio key={id} value={id} className={sourceClass}>
                {t(`launcherImport.source.${id}`)}
              </Radio>
            ))}
          </RadioGroup>
          <Button type="button" onPress={choose} className={`${sourceClass} bg-surface-1`}>
            <IconFolder aria-hidden className="size-4" />
            {t("launcherImport.chooseFolder")}
          </Button>
        </div>
        {list.isPending && (
          <div aria-busy="true" className="flex flex-col gap-3">
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} className={`h-9 w-full ${onPanel}`} />
            ))}
          </div>
        )}
        {listError && (
          <div role="alert">
            <FailureNotice error={listError} />
          </div>
        )}
        {list.data && rows.length === 0 && <p className="text-text-muted">{t("launcherImport.empty")}</p>}
        {rows.length > 0 && (
          <ul className="scroll-thin flex max-h-96 flex-col gap-2 overflow-y-auto">
            {rows.map((row) => {
              const selected = picked.has(row.id);
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
                    {row.mcVersion !== "" && (
                      <span className="tabular shrink-0 text-sm text-text-muted">
                        {row.mcVersion} · {t(`create.tab.${row.loader}`)}
                      </span>
                    )}
                    {duplicated && (
                      <span className="ml-auto flex shrink-0 items-center gap-1 text-sm text-warning">
                        <IconWarning aria-hidden className="size-4" />
                        {t("launcherImport.duplicate")}
                      </span>
                    )}
                  </div>
                  {row.unsupported !== null && (
                    <p className="pl-6 text-sm text-text-muted">
                      {t(`launcherImport.unsupported.${row.unsupported.code}`, {
                        detail: row.unsupported.detail ?? "",
                      })}
                    </p>
                  )}
                  {selected && (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 pl-6">
                      {row.available.map((kind) => (
                        <Checkbox
                          key={kind}
                          isSelected={picked.get(row.id)?.includes(kind) ?? false}
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
