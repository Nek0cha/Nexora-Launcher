import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { Button, Radio, RadioGroup } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { resolveSyncProposal } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { ConflictView } from "../../bindings/ConflictView";
import type { PendingView } from "../../bindings/PendingView";
import type { Resolution } from "../../bindings/Resolution";
import type { SingleProposal } from "../../bindings/SingleProposal";
import type { SyncFieldValue } from "../../bindings/SyncFieldValue";
import type { SyncMember } from "../../bindings/SyncMember";
import { fieldLabel } from "../../features/sync/fieldLabels";
import { formatValue } from "../../features/sync/formatValue";
import {
  type ResolveCall,
  bulkCalls,
  conflictCalls,
  runSerially,
  scopeProposals,
  singleCalls,
} from "../../features/sync/proposals";
import { SYNC_KEY, useSyncProfileValues, useSyncProposals } from "../../features/sync/useInstanceSync";
import { FailureNotice } from "../FailureNotice";
import { RadioMark } from "../RadioMark";
import { useToast } from "../toast/useToast";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const primaryButton = [
  "inline-flex h-8 items-center justify-center rounded-md bg-text-strong px-3 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-40",
  ring,
].join(" ");

const secondaryButton = [
  "inline-flex h-8 items-center justify-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50 data-disabled:hover:bg-transparent",
  ring,
].join(" ");

// 控えめな操作。面は乗せたときだけ現れる
const quietButton = [
  "inline-flex h-8 items-center justify-center rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

const radioClass = [
  "group flex cursor-default items-center gap-2.5 rounded-md py-1 text-sm text-text",
  "data-disabled:cursor-not-allowed data-disabled:opacity-60",
  "data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-accent",
].join(" ");

type Scope = {
  profileId: string;
  members: SyncMember[];
  /** インスタンスのカードではそのインスタンスの ID。「共通設定」の画面では null */
  instanceId: string | null;
};

/**
 * ゲームの終了後に見つかった、設定の変更（提案）の件数と確認（spec 5.4、5.5）。
 * 0 件なら何も出さない。count を渡したとき（インスタンスのカード）は、開くまで一覧を取りに行かない。
 */
export function ProposalsSection({ profileId, members, instanceId, count }: {
  profileId: string;
  members: SyncMember[];
  instanceId?: string;
  count?: number;
}) {
  const { t } = useTranslation();
  const listId = useId();
  const [open, setOpen] = useState(false);
  const scope: Scope = { profileId, members, instanceId: instanceId ?? null };
  const proposals = useSyncProposals(profileId, open || count === undefined);
  // 値の無い表示がちらつかないよう、共通設定の値が届いてから一覧を出す
  const values = useSyncProfileValues(profileId, open);
  const scoped = proposals.data ? scopeProposals(proposals.data, scope.instanceId) : null;
  const total = count ?? (scoped ? scoped.single.length + scoped.conflicts.length : 0);
  if (total === 0) return null;

  const summary =
    scope.instanceId === null
      ? t("sync.proposals.countProfile", { count: total })
      : t("sync.proposals.countInstance", { count: total });

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <span
          data-testid="proposal-badge"
          aria-hidden
          className="inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-accent px-1.5 text-xs font-semibold tabular-nums text-accent-ink"
        >
          {total}
        </span>
        <p className="min-w-0 flex-1 text-sm text-text">{summary}</p>
        <Button
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          onPress={() => setOpen((v) => !v)}
          className={secondaryButton}
        >
          {open ? t("sync.proposals.hide") : t("sync.proposals.review")}
        </Button>
      </div>
      {open && (
        <div id={listId}>
          {(proposals.error ?? values.error) && !(scoped && values.data) && (
            <div role="alert">
              <FailureNotice error={toErrorPayload(proposals.error ?? values.error)} />
            </div>
          )}
          {scoped && values.data && <ProposalList view={scoped} values={values.data} scope={scope} />}
        </div>
      )}
    </div>
  );
}

/** 項目ごとの「いまの共通設定の値 → 変えた値」と、3 つの選択肢。競合は値を 1 つ選ばせる */
export function ProposalList({ view, values, scope }: { view: PendingView; values: SyncFieldValue[]; scope: Scope }) {
  const { t, i18n } = useTranslation();
  const client = useQueryClient();
  const toast = useToast();
  const noteId = useId();

  // 解決はいつも直列。進んでいる間は、ほかの選択肢を押せない
  const resolve = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: (calls: ResolveCall[]) =>
      runSerially(calls, (c) => resolveSyncProposal(c.instanceId, c.fieldId, c.resolution, c.chosenValue)),
    onSuccess: (_, calls) => {
      if (calls.some((c) => c.resolution === "applyToProfile"))
        toast.show({ kind: "success", message: t("sync.proposals.applied") });
    },
    onError: toast.showError,
    // 途中で失敗しても、済んだ分を映すために取り直す
    // 取り直しが終わるまで進行中のままにする（解決した項目が一瞬、押せる状態で戻らないように）
    onSettled: () => client.invalidateQueries({ queryKey: SYNC_KEY }),
  });

  const ctx: ItemContext = {
    scope,
    busy: resolve.isPending,
    noteId,
    current: (fieldId) => {
      const found = values.find((v) => v.fieldId === fieldId);
      return found ? formatValue(fieldId, found.value, t) : t("sync.proposals.notSet");
    },
    format: (fieldId, value) => formatValue(fieldId, value, t),
    label: (fieldId) => fieldLabel(fieldId, i18n),
    names: (ids) =>
      ids
        .map((id) => scope.members.find((m) => m.id === id)?.name)
        .filter((n): n is string => n !== undefined)
        .join(t("sync.card.separator")),
    run: (calls) => {
      if (calls.length > 0) resolve.mutate(calls);
    },
  };

  const bulk = view.single.length >= 2 || (view.single.length >= 1 && view.conflicts.length >= 1);

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-line bg-surface-1 p-4">
      <p className="text-sm text-text-muted">{t("sync.proposals.intro")}</p>
      {bulk && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-line pb-3">
          <Button
            onPress={() => ctx.run(bulkCalls(view, scope.instanceId))}
            isDisabled={resolve.isPending}
            className={primaryButton}
          >
            {t("sync.proposals.applyAll", { count: view.single.length })}
          </Button>
          {view.conflicts.length > 0 && (
            <p className="min-w-0 flex-1 basis-60 text-xs text-text-muted">{t("sync.proposals.conflictsExcluded")}</p>
          )}
        </div>
      )}
      <ul aria-label={t("sync.proposals.list")} className="flex flex-col divide-y divide-line">
        {view.single.map((s) => (
          <SingleItem key={s.fieldId} item={s} ctx={ctx} />
        ))}
        {view.conflicts.map((c) => (
          <ConflictItem key={c.fieldId} conflict={c} ctx={ctx} />
        ))}
      </ul>
      <p id={noteId} className="text-xs text-text-muted">
        {t("sync.proposals.ignoreNote")}
      </p>
    </div>
  );
}

type ItemContext = {
  scope: Scope;
  busy: boolean;
  /** 「無視」の注記の ID */
  noteId: string;
  current: (fieldId: string) => string;
  format: (fieldId: string, value: string) => string;
  label: (fieldId: string) => string;
  names: (instanceIds: string[]) => string;
  run: (calls: ResolveCall[]) => void;
};

/** 「いまの共通設定 70° → このインスタンス 85°」 */
function Change({ current, who, value }: { current: string; who: string; value: string }) {
  const { t } = useTranslation();
  return (
    <p className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-sm">
      <span className="text-text-muted">{t("sync.proposals.current")}</span>
      <span className="text-text tabular-nums">{current}</span>
      <span aria-hidden className="px-0.5 text-text-faint">
        →
      </span>
      <span className="text-text-muted">{who}</span>
      <span className="font-medium text-text-strong tabular-nums">{value}</span>
    </p>
  );
}

function SingleItem({ item, ctx }: { item: SingleProposal; ctx: ItemContext }) {
  const { t } = useTranslation();
  const headingId = useId();
  const onCard = ctx.scope.instanceId !== null;
  const names = ctx.names(item.instanceIds);
  const choose = (resolution: Resolution) => ctx.run(singleCalls(item, resolution, ctx.scope.instanceId));
  return (
    <li aria-labelledby={headingId} className="flex flex-col gap-2 py-3 first:pt-0 last:pb-0">
      <h4 id={headingId} className="text-sm font-medium text-text-strong">
        {ctx.label(item.fieldId)}
      </h4>
      <Change
        current={ctx.current(item.fieldId)}
        who={onCard ? t("sync.proposals.thisInstance") : names}
        value={ctx.format(item.fieldId, item.value)}
      />
      <div className="flex flex-wrap items-center gap-2">
        {/* 3 つの選択肢に優劣を付けない。「無視」だけは控えめにする */}
        <Button onPress={() => choose("applyToProfile")} isDisabled={ctx.busy} className={secondaryButton}>
          {t("sync.proposals.applyToProfile")}
        </Button>
        <Button onPress={() => choose("keepForInstance")} isDisabled={ctx.busy} className={secondaryButton}>
          {onCard ? t("sync.proposals.keepForInstance") : t("sync.proposals.keepFor", { names })}
        </Button>
        <Button
          onPress={() => choose("ignore")}
          isDisabled={ctx.busy}
          aria-describedby={ctx.noteId}
          className={quietButton}
        >
          {t("sync.proposals.ignore")}
        </Button>
      </div>
    </li>
  );
}

function ConflictItem({ conflict, ctx }: { conflict: ConflictView; ctx: ItemContext }) {
  const { t } = useTranslation();
  const headingId = useId();
  const noteId = useId();
  const [chosen, setChosen] = useState<string | null>(null);
  const label = ctx.label(conflict.fieldId);
  // 同じ値を出したインスタンスは 1 つの選択肢にまとめる
  const byValue = new Map<string, string[]>();
  for (const e of conflict.entries) byValue.set(e.value, [...(byValue.get(e.value) ?? []), e.instanceId]);

  return (
    <li aria-labelledby={headingId} className="flex flex-col gap-2 py-3 first:pt-0 last:pb-0">
      <h4 id={headingId} className="text-sm font-medium text-text-strong">
        {label}
      </h4>
      <p className="text-sm text-text-muted">{t("sync.proposals.conflict")}</p>
      <p className="flex flex-wrap items-baseline gap-x-2 text-sm">
        <span className="text-text-muted">{t("sync.proposals.current")}</span>
        <span className="text-text tabular-nums">{ctx.current(conflict.fieldId)}</span>
      </p>
      <RadioGroup
        aria-label={t("sync.proposals.conflictChoices", { name: label })}
        value={chosen}
        onChange={setChosen}
        isDisabled={ctx.busy}
        className="flex flex-col gap-0.5"
      >
        {[...byValue].map(([value, ids]) => (
          <Radio key={value} value={value} className={radioClass}>
            <RadioMark />
            <span className="font-medium text-text-strong tabular-nums">{ctx.format(conflict.fieldId, value)}</span>
            <span className="text-text-muted"> {ctx.names(ids)}</span>
          </Radio>
        ))}
      </RadioGroup>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          onPress={() => chosen !== null && ctx.run(conflictCalls(conflict, { choose: chosen }, ctx.scope.instanceId))}
          isDisabled={ctx.busy || chosen === null}
          aria-describedby={noteId}
          className={primaryButton}
        >
          {t("sync.proposals.applyChosen")}
        </Button>
        <Button
          onPress={() => ctx.run(conflictCalls(conflict, "ignore", ctx.scope.instanceId))}
          isDisabled={ctx.busy}
          aria-describedby={ctx.noteId}
          className={quietButton}
        >
          {t("sync.proposals.ignore")}
        </Button>
      </div>
      <p id={noteId} className="text-xs text-text-muted">
        {t("sync.proposals.conflictNote")}
      </p>
    </li>
  );
}
