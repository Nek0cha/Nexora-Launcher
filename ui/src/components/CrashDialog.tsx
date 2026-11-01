import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button, Checkbox, Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import IconCheck from "~icons/ph/check";
import IconLog from "~icons/ph/file-text";
import { diagnoseCrash, setContentEnabled } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { CrashCandidate } from "../bindings/CrashCandidate";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import { initialChecked } from "../features/crash/crashSelection";
import { type CrashEntry, useCrashQueue } from "../features/crash/useCrashQueue";
import { useInstances } from "../features/instances/useInstances";
import { FailureNotice } from "./FailureNotice";
import { checkboxClass, checkboxMark, quietTrigger, secondaryButton, submitButton } from "./mrpack/dialogStyles";
import { centeredOverlayClass, modalClass } from "./modal";

const reasonKey = (reason: CrashCandidate["reasons"][number]) => `crash.reason.${reason}` as const;

/** 候補の 1 行。理由は強い順に並べ、足りない側の MOD があればその ID を添える。 */
function Reasons({ candidate }: { candidate: CrashCandidate }) {
  const { t } = useTranslation();
  const text = candidate.reasons
    .map((reason) => t(reasonKey(reason), { missing: candidate.missing ?? "" }).trim())
    .join(" / ");
  return <span className="truncate text-xs text-text-muted">{text}</span>;
}

/** 診断の結果。開くたびに作り直し、チェックの初期値を確度から決める。 */
function Body({ entry, onClose }: { entry: CrashEntry; onClose: () => void }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { data: instances } = useInstances();
  const diagnosis = useQuery({
    queryKey: ["crashDiagnosis", entry.instanceId, entry.launchedAt],
    queryFn: () => diagnoseCrash(entry.instanceId, entry.launchedAt),
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
  });
  // 診断の失敗は、候補なしと同じ見せ方にする
  const loaded = diagnosis.isSuccess || diagnosis.isError;
  const found = diagnosis.data?.candidates ?? [];
  const [checked, setChecked] = useState<Set<string> | null>(null);
  const [failures, setFailures] = useState<Record<string, ErrorPayload>>({});
  const [removed, setRemoved] = useState<Set<string>>(new Set());
  const shown = found.filter((c) => !removed.has(c.relPath));
  const selected = checked ?? new Set(initialChecked(found));
  const name = instances?.find((i) => i.id === entry.instanceId)?.name ?? entry.instanceId;

  const disable = useMutation({
    mutationFn: async () => {
      const done = new Set<string>();
      const failed: Record<string, ErrorPayload> = {};
      // 1 件ずつ順に行う（インスタンスのロックを取り合わない）。1 件の失敗で残りを止めない
      for (const c of shown.filter((x) => selected.has(x.relPath))) {
        try {
          await setContentEnabled(entry.instanceId, "mod", c.relPath, false);
          done.add(c.relPath);
        } catch (error) {
          failed[c.relPath] = toErrorPayload(error);
        }
      }
      return { done, failed };
    },
    onSuccess: ({ done, failed }) => {
      setRemoved((prev) => new Set([...prev, ...done]));
      setFailures(failed);
      if (Object.keys(failed).length === 0) onClose();
    },
  });

  const openLog = () => {
    navigate(`/instances/${entry.instanceId}`, { state: { tab: "logs" } });
    onClose();
  };

  return (
    <Dialog role="alertdialog" aria-label={t("crash.title")} className="flex flex-col gap-4 p-5 outline-hidden">
      <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
        {t("crash.title")}
      </Heading>
      <p className="-mt-2 truncate text-sm text-text-muted">{t("crash.instance", { name })}</p>
      {loaded && shown.length === 0 && <p className="text-sm text-text">{t("crash.unknown")}</p>}
      {shown.length > 0 && (
        <ul className="scroll-thin flex max-h-72 flex-col gap-2 overflow-auto">
          {shown.map((c) => (
            <li key={c.relPath} className="flex flex-col gap-1" title={c.evidence}>
              <Checkbox
                isSelected={selected.has(c.relPath)}
                onChange={(on) => {
                  const next = new Set(selected);
                  if (on) next.add(c.relPath);
                  else next.delete(c.relPath);
                  setChecked(next);
                }}
                className={checkboxClass}
              >
                <span aria-hidden className={checkboxMark}>
                  <IconCheck className="size-3 opacity-0 group-data-selected:opacity-100" />
                </span>
                <span className="truncate">{c.name}</span>
                {c.confidence === "low" && <span className="text-xs text-text-muted">{t("crash.lowConfidence")}</span>}
              </Checkbox>
              <Reasons candidate={c} />
              {failures[c.relPath] && (
                <div role="alert">
                  <FailureNotice error={failures[c.relPath] ?? null} />
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-center justify-between gap-2 pt-1">
        <Button onPress={openLog} className={quietTrigger}>
          <IconLog aria-hidden className="size-4" />
          {t("crash.openLog")}
        </Button>
        <div className="flex gap-2">
          <Button autoFocus onPress={onClose} className={secondaryButton}>
            {t("crash.close")}
          </Button>
          {shown.length > 0 && (
            <Button
              onPress={() => disable.mutate()}
              isPending={disable.isPending}
              isDisabled={selected.size === 0}
              className={submitButton}
            >
              {t("crash.disable")}
            </Button>
          )}
        </div>
      </div>
    </Dialog>
  );
}

/**
 * 異常終了の直後に、原因の MOD の候補を見せて、選んだものを無効化するダイアログ。アプリに 1 つだけ置く。
 * 診断が済むまでは開かない。複数のインスタンスが続けて落ちたときは、1 つずつ順に出す。
 */
export function CrashDialog() {
  const { head, dismiss } = useCrashQueue();
  // 閉じる動きの間も、最後に見せた内容を出し続ける
  const [last, setLast] = useState<CrashEntry | null>(null);
  if (head && (last?.instanceId !== head.instanceId || last.launchedAt !== head.launchedAt)) setLast(head);
  const entry = head ?? last;

  return (
    <ModalOverlay
      isOpen={head !== null}
      onOpenChange={(open) => {
        if (!open) dismiss();
      }}
      isDismissable
      className={centeredOverlayClass}
    >
      <Modal className={`${modalClass} max-w-md`}>
        {entry && <Body key={`${entry.instanceId}:${entry.launchedAt}`} entry={entry} onClose={dismiss} />}
      </Modal>
    </ModalOverlay>
  );
}
