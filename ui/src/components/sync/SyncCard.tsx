import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useMemo, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { leaveSyncProfile, setSyncOverride } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { Instance } from "../../bindings/Instance";
import type { InstanceSync } from "../../bindings/InstanceSync";
import type { SyncField } from "../../bindings/SyncField";
import type { SyncFieldValue } from "../../bindings/SyncFieldValue";
import { fieldLabel, hasFieldLabel } from "../../features/sync/fieldLabels";
import { formatValue } from "../../features/sync/formatValue";
import { applicabilityKey, isKnownFieldId, reasonKey } from "../../features/sync/syncText";
import { SYNC_KEY, useInstanceSync, useSyncFields } from "../../features/sync/useInstanceSync";
import { FailureNotice } from "../FailureNotice";
import { Notice } from "../settingsLayout";
import { useToast } from "../toast/useToast";
import { InstanceChip } from "./InstanceChip";
import { JoinSyncDialog } from "./JoinSyncDialog";
import { ProposalsSection } from "./ProposalList";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const secondaryButton = [
  "inline-flex h-9 items-center justify-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50 data-pending:cursor-progress",
  ring,
].join(" ");

// 行の中の控えめな操作。面は乗せたときだけ現れる（一覧の行の操作と同じ）
const rowButton = [
  "inline-flex h-8 shrink-0 items-center rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

/**
 * インスタンスの設定のタブの先頭に置く「設定の同期」（spec 3.1 の入口 1）。
 * 未参加なら揃えるためのボタンだけ、参加済みなら共通設定の名前、仲間、反映の状態、例外、離脱を出す。
 */
export function SyncCard({ instance, running }: { instance: Instance; running: boolean }) {
  const { t } = useTranslation();
  const headingId = useId();
  const { data, error } = useInstanceSync(instance.id);

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2">
      <h2 id={headingId} className="text-xs font-medium tracking-[0.08em] text-text-muted">
        {t("sync.card.title")}
      </h2>
      {/* 下の線は置かない。続く設定の欄の上の線が区切りになる */}
      <div className="flex flex-col gap-3 border-t border-line pt-3">
        {!data && error && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(error)} />
          </div>
        )}
        {data &&
          (data.profile ? (
            <Joined instanceId={instance.id} sync={data} running={running} />
          ) : (
            <NotJoined instanceId={instance.id} sync={data} running={running} />
          ))}
      </div>
    </section>
  );
}

function NotJoined({ instanceId, sync, running }: { instanceId: string; sync: InstanceSync; running: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <div className="flex flex-col items-start gap-3">
      <Button onPress={() => setOpen(true)} className={secondaryButton}>
        {t("sync.card.start")}
      </Button>
      <JoinSyncDialog
        instanceId={instanceId}
        hasOptions={sync.hasOptions}
        applicability={sync.applicability}
        running={running}
        isOpen={open}
        onOpenChange={setOpen}
      />
    </div>
  );
}

function Joined({ instanceId, sync, running }: { instanceId: string; sync: InstanceSync; running: boolean }) {
  const { t, i18n } = useTranslation();
  const client = useQueryClient();
  const toast = useToast();
  const fields = useSyncFields().data ?? [];
  const date = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }),
    [i18n.language],
  );

  const leave = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: () => leaveSyncProfile(instanceId),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: SYNC_KEY });
      toast.show({ kind: "success", message: t("sync.card.left") });
    },
    onError: toast.showError,
  });

  const release = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: (fieldId: string) => setSyncOverride(instanceId, fieldId, null),
    onSuccess: () => void client.invalidateQueries({ queryKey: SYNC_KEY }),
    onError: toast.showError,
  });

  const others = sync.members.filter((m) => m.id !== instanceId).map((m) => m.name);
  const last = sync.lastApplied;
  const applicable = sync.applicability === "ready";
  // 版やファイルの理由で適用できないときは、同じ理由の「前回のスキップ」を重ねて出さない
  const skipCoveredByApplicability =
    !applicable && last?.state === "skipped" && last.reason !== "nothingToApply";

  return (
    <>
      <div className="flex flex-col gap-0.5">
        <p className="text-text">{sync.profile?.name}</p>
        {others.length > 0 && (
          <ul className="flex flex-wrap gap-1.5 py-1">
            {others.map((name) => (
              <li key={name} className="max-w-full">
                <InstanceChip name={name} />
              </li>
            ))}
          </ul>
        )}
        {last?.state === "applied" && last.atMs !== null && (
          <p className="text-sm text-text-muted">
            {t("sync.card.lastApplied", { date: date.format(last.atMs) })}
          </p>
        )}
        {!last && <p className="text-sm text-text-muted">{t("sync.card.neverApplied")}</p>}
      </div>

      {!applicable && (
        <Notice tone="warning">
          {t("sync.card.notApplicable", {
            reason: t(applicabilityKey(sync.applicability as Exclude<typeof sync.applicability, "ready">)),
          })}
        </Notice>
      )}
      {last?.state === "failed" && (
        <Notice tone="warning">{t("sync.card.lastFailed", { reason: t(reasonKey(last.reason)) })}</Notice>
      )}
      {last?.state === "skipped" && !skipCoveredByApplicability && (
        <Notice tone="info">{t("sync.card.lastSkipped", { reason: t(reasonKey(last.reason)) })}</Notice>
      )}
      {/* 適用できる状態のときだけ「反映されます」と言う。反映済み、または反映するものが無いなら何も言わない。
          実行中は、設定の保存の上に出る「次の起動から反映されます」と同じ内容なので、ここでは言わない */}
      {applicable && !sync.upToDate && last?.reason !== "nothingToApply" && !running && (
        <Notice tone="info">{t("sync.card.nextLaunch")}</Notice>
      )}

      {sync.profile && (
        <ProposalsSection
          profileId={sync.profile.id}
          members={sync.members}
          instanceId={instanceId}
          count={sync.pendingCount}
        />
      )}

      {sync.overrides.length > 0 && (
        <Overrides
          overrides={sync.overrides}
          fields={fields}
          onRelease={(fieldId) => release.mutate(fieldId)}
          isDisabled={release.isPending || leave.isPending}
        />
      )}

      {/* 外れる操作は確認を挟まない。外れても設定ファイルは変わらず、また参加できる */}
      <div className="pt-1">
        <Button
          onPress={() => leave.mutate()}
          isPending={leave.isPending}
          isDisabled={release.isPending}
          className={secondaryButton}
        >
          {t("sync.card.leave")}
        </Button>
      </div>
    </>
  );
}

function Overrides({ overrides, fields, onRelease, isDisabled }: {
  overrides: SyncFieldValue[];
  fields: SyncField[];
  onRelease: (fieldId: string) => void;
  isDisabled: boolean;
}) {
  const { t, i18n } = useTranslation();
  const headingId = useId();
  return (
    <div className="flex flex-col gap-1">
      <h3 id={headingId} className="text-sm text-text">
        {t("sync.card.overrides")}
      </h3>
      <ul aria-labelledby={headingId} className="flex flex-col divide-y divide-line">
        {overrides.map(({ fieldId: id, value }) => {
          const name = fieldLabel(id, i18n);
          const known = isKnownFieldId(id, fields);
          return (
            <li key={id} className="flex min-h-10 items-center justify-between gap-4">
              <span className="flex min-w-0 items-baseline gap-2 text-sm">
                <span className="truncate text-text-muted">{name}</span>
                {/* 名前の無い項目は、値からも何の設定か分からないので出さない */}
                {hasFieldLabel(id, i18n) && (
                  <span className="shrink-0 text-text tabular-nums">{formatValue(id, value, t)}</span>
                )}
              </span>
              {/* 解除のコマンドには、知っている項目の ID だけを送る */}
              {known && (
                <Button
                  aria-label={t("sync.card.releaseLabel", { name })}
                  onPress={() => onRelease(id)}
                  isDisabled={isDisabled}
                  className={rowButton}
                >
                  {t("sync.card.release")}
                </Button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
