import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Button, Tab, TabList, TabPanel, Tabs } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import IconUpdate from "~icons/ph/arrow-up";
import IconCheck from "~icons/ph/arrows-clockwise";
import IconDrop from "~icons/ph/download-simple";
import IconPlus from "~icons/ph/plus";
import IconOffline from "~icons/ph/wifi-slash";
import { onFilesDropped } from "../api/dragDrop";
import { toErrorPayload } from "../api/ipc";
import type { ContentItem } from "../bindings/ContentItem";
import type { ContentKind } from "../bindings/ContentKind";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import type { Instance } from "../bindings/Instance";
import type { ItemFailure } from "../bindings/ItemFailure";
import type { RejectReason } from "../bindings/RejectReason";
import type { VersionChoice } from "../bindings/VersionChoice";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ContentFilter, ContentSearchField } from "../components/content/ContentFilter";
import { ContentList, type RowUpdates } from "../components/content/ContentList";
import { ROW_SIZE } from "../components/content/ContentRow";
import { FailureNotice } from "../components/FailureNotice";
import { Notice } from "../components/settingsLayout";
import { useToast } from "../components/toast/useToast";
import {
  applyFilter,
  dependentsOf,
  emptyFilter,
  type FilterState,
  isMetadataFiltered,
  type StatusKey,
  sortByName,
} from "../features/content/contentFilter";
import {
  useAddFiles,
  useCategories,
  useContentList,
  useIdentifyContent,
  useRemove,
  useSetEnabled,
} from "../features/content/useContent";
import {
  useChangeContentVersion,
  useContentUpdates,
  useRecheckUpdates,
  useUpdateContent,
  useUpdateFailures,
} from "../features/modrinth/useModrinth";

const kinds: ContentKind[] = ["mod", "resourcepack", "shader"];

// 種類ごとに受け付ける拡張子（spec 5.6）
const extensions: Record<ContentKind, string> = { mod: ".jar", resourcepack: ".zip", shader: ".zip" };

// 対象外の行を一覧の下に出すとき、この行数を超えたらその中でスクロールする
const EXCLUDED_ROWS = 4;

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// サブタブ。詳細の下線のタブと区別できるよう、選んだものに面を敷く
const subTabClass = [
  "inline-flex h-8 cursor-default items-center gap-2 rounded-md px-3 text-sm text-text-muted outline-none",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text selected:bg-surface-3 selected:text-text-strong",
  ring,
].join(" ");

// 本文の中の控えめな文字のボタン（失敗の説明の「詳細」と同じ形）
const textButton = [
  "rounded-sm text-sm text-text-muted underline underline-offset-2",
  "data-hovered:text-text data-pressed:text-text-strong",
  ring,
].join(" ");

// 一覧の上の行のボタン（確認のダイアログの「キャンセル」と同じ、枠だけのボタン）。高さは名前の入力に揃える
const addLink = [
  "inline-flex h-9 shrink-0 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  ring,
].join(" ");

// MOD のサブタブの状態の札。更新の確認の結果で「更新あり」も選べる
const modStatuses: StatusKey[] = ["enabled", "disabled", "dependency", "update"];

const noRows: ReadonlySet<string> = new Set();

/** 更新の確認の失敗のうち、控えめに「あとで確認します」とだけ示すもの（オフラインと 429）。 */
function isQuietCheckError(error: ErrorPayload): boolean {
  return error.code === "net.offline" || (error.code === "net.http_status" && error.params.status === 429);
}

/** MOD のサブタブの、更新の部品に渡すもの。 */
type UpdateControls = {
  rows: RowUpdates;
  /** 更新が見つかった行の relPath（状態の絞り込みの「更新あり」に使う） */
  updatable: ReadonlySet<string>;
  count: number;
  checking: boolean;
  onCheck: () => void;
  onUpdateAll: () => void;
  /** 更新や確認を始められないとき（更新の最中、起動中など） */
  disabled: boolean;
  /** 確認の失敗。オフラインと 429 は控えめに出す */
  checkError: ErrorPayload | null;
  /** 直前の更新で失敗した MOD。閉じるまで残す */
  failures: ItemFailure[] | null;
  onDismissFailures: () => void;
};

// 更新の行のボタン。上の行のボタンと同じ枠だけの形で、一段小さくする
const barButton = [
  "inline-flex h-8 shrink-0 items-center gap-2 rounded-md border border-line-control px-2.5 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3 disabled:opacity-50 disabled:hover:bg-transparent",
  ring,
].join(" ");

type Dialog =
  | { type: "disable"; item: ContentItem; dependents: ContentItem[] }
  | { type: "remove"; item: ContentItem; dependents: ContentItem[] }
  | { type: "conflicts"; kind: ContentKind; names: string[]; paths: string[] };

const displayName = (item: ContentItem) => item.name ?? item.fileName;

/** パスの最後の部分（ファイル名）。Windows の区切りにも対応する。 */
function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/**
 * インスタンスの詳細の「コンテンツ」タブ（spec 5.2〜5.7）。MOD、リソースパック、シェーダーのサブタブに分け、
 * 有効無効、削除、ドロップによる追加を行う。起動中、インストール中、複製中（`busy`）は変更できない。
 */
export function ContentTab({ instance, busy: locked }: { instance: Instance; busy: boolean }) {
  const { t } = useTranslation();
  const toast = useToast();
  const [kind, setKind] = useState<ContentKind>("mod");
  // 絞り込みはサブタブごとに持つ。サブタブを切り替えても残す
  const [filters, setFilters] = useState<Record<ContentKind, FilterState>>({
    mod: emptyFilter,
    resourcepack: emptyFilter,
    shader: emptyFilter,
  });
  const [dragging, setDragging] = useState(false);
  // ダイアログの中身は、閉じる動きの間も残しておく
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const lists = {
    mod: useContentList(instance.id, "mod"),
    resourcepack: useContentList(instance.id, "resourcepack"),
    shader: useContentList(instance.id, "shader"),
  };
  const identify = useIdentifyContent(instance.id);
  const setEnabled = useSetEnabled(instance.id);
  const remove = useRemove(instance.id);
  const add = useAddFiles(instance.id);

  // 更新の確認は、ローダーのあるインスタンスで、MOD のサブタブを開いたときに 1 回だけ行う（キャッシュは core が持つ）
  const canUpdate = instance.loader !== "vanilla";
  const [modOpened, setModOpened] = useState(kind === "mod");
  useEffect(() => {
    if (kind === "mod") setModOpened(true);
  }, [kind]);
  const updates = useContentUpdates(instance.id, canUpdate && modOpened);
  const recheck = useRecheckUpdates(instance.id);
  const update = useUpdateContent(instance.id);
  const change = useChangeContentVersion(instance.id);
  // 更新している行と、直前の更新で失敗した MOD
  const [updating, setUpdating] = useState<ReadonlySet<string>>(noRows);
  // 失敗の一覧はタブを開き直しても残す
  const [failures, setFailures] = useUpdateFailures(instance.id);

  // 開いたときに 1 回だけ特定する（StrictMode で効果が 2 回走っても 1 回にする）
  const identified = useRef(false);
  useEffect(() => {
    if (identified.current) return;
    identified.current = true;
    identify.mutate();
  }, [identify.mutate]);

  // 一覧の行とドロップの受け口は 1 度だけ作るので、最新の値はここから読む
  const latest = useRef({
    locked,
    kind,
    dialogOpen,
    mods: [] as ContentItem[],
    toast,
    t,
    setEnabled: setEnabled.mutate,
    add: add.mutate,
    identify: identify.mutate,
  });
  useLayoutEffect(() => {
    latest.current = {
      // 更新と版の切り替えの最中は、インスタンスが他のコンテンツの変更を受け付けない
      locked: locked || update.isPending || change.isPending,
      kind,
      dialogOpen,
      mods: lists.mod.data ?? [],
      toast,
      t,
      setEnabled: setEnabled.mutate,
      add: add.mutate,
      identify: identify.mutate,
    };
  });
  // 追加の最中のドロップは受けない。状態の反映を待たずに止めるため ref で持つ
  const adding = useRef(false);

  const toggle = useCallback((item: ContentItem) => {
    latest.current.setEnabled(
      { kind: item.kind, relPath: item.relPath, enabled: !item.enabled },
      { onError: latest.current.toast.showError },
    );
  }, []);

  const addFiles = useCallback((target: ContentKind, paths: string[], overwrite: boolean) => {
    const { t, toast } = latest.current;
    adding.current = true;
    latest.current.add(
      { kind: target, paths, overwrite },
      {
        onSuccess: (report) => {
          // 入れたファイルにも、開いている間に名前、アイコン、依存を付ける
          if (report.added.length > 0) latest.current.identify();
          for (const r of report.rejected)
            toast.show({ kind: "error", message: t("content.reject.message", { name: r.name, reason: rejectText(t, r.reason, target) }) });
          if (!overwrite && report.conflicts.length > 0) {
            // 同じ名前があったファイルだけを、了承されたら上書きで入れ直す
            const names = new Set(report.conflicts);
            setDialog({ type: "conflicts", kind: target, names: report.conflicts, paths: paths.filter((p) => names.has(baseName(p))) });
            setDialogOpen(true);
          }
        },
        onError: toast.showError,
        onSettled: () => {
          adding.current = false;
        },
      },
    );
  }, []);

  // 行の操作。行を描き直さずに済むよう、関数は作り直さない
  const onToggle = useCallback(
    (item: ContentItem) => {
      if (latest.current.locked) return;
      // 依存の警告は MOD を無効にするときだけ。有効にするときは警告しない
      if (item.kind === "mod" && item.enabled) {
        const dependents = dependentsOf(latest.current.mods, item);
        if (dependents.length > 0) {
          setDialog({ type: "disable", item, dependents });
          setDialogOpen(true);
          return;
        }
      }
      toggle(item);
    },
    [toggle],
  );

  const onRemove = useCallback((item: ContentItem) => {
    if (latest.current.locked) return;
    const dependents = item.kind === "mod" ? dependentsOf(latest.current.mods, item) : [];
    setDialog({ type: "remove", item, dependents });
    setDialogOpen(true);
  }, []);

  // 更新。1 件でも「すべて更新」でも 1 つのタスクで行い、終わったら結果をまとめて知らせる
  const runUpdate = useCallback(
    async (relPaths: string[]) => {
      if (latest.current.locked || relPaths.length === 0) return;
      // 状態の反映を待たずに二重の開始を止める
      latest.current.locked = true;
      const { t, toast } = latest.current;
      setUpdating(new Set(relPaths));
      setFailures(null);
      try {
        const outcome = await update.mutateAsync(relPaths);
        const report = outcome.report;
        if (report && report.failures.length > 0) setFailures(report.failures);
        if (outcome.state === "succeeded") {
          if (report && report.updated > 0)
            toast.show({ kind: "success", message: t("modrinth.updated", { count: report.updated }) });
        } else if (outcome.state === "cancelled") {
          toast.show({ kind: "info", message: t("modrinth.updateCancelled") });
        } else {
          toast.showError(outcome.error ?? new Error("update task failed"));
        }
      } catch (error) {
        toast.showError(error);
      } finally {
        setUpdating(noRows);
      }
    },
    [setFailures, update.mutateAsync],
  );

  const onUpdate = useCallback((item: ContentItem) => void runUpdate([item.relPath]), [runUpdate]);

  // 版の切り替え。更新と同じく 1 つのタスクで行い、その間は行を「更新中…」にする
  const onChangeVersion = useCallback(
    async (item: ContentItem, choice: VersionChoice) => {
      if (latest.current.locked) return;
      latest.current.locked = true;
      const { t, toast } = latest.current;
      const name = displayName(item);
      setUpdating(new Set([item.relPath]));
      try {
        const outcome = await change.mutateAsync({ relPath: item.relPath, versionId: choice.versionId });
        if (outcome.state === "succeeded")
          toast.show({ kind: "success", message: t("modrinth.versionChanged", { name, version: choice.versionNumber }) });
        else if (outcome.state === "cancelled") toast.show({ kind: "info", message: t("modrinth.versionChangeCancelled") });
        else toast.showError(outcome.error ?? new Error("version change task failed"));
      } catch (error) {
        toast.showError(error);
      } finally {
        setUpdating(noRows);
      }
    },
    [change.mutateAsync],
  );

  const check = updates.data;
  const available = useMemo(() => new Map((check?.updates ?? []).map((u) => [u.relPath, u])), [check]);
  const noCompatible = useMemo(() => new Set(check?.noCompatible ?? []), [check]);
  const instanceId = instance.id;
  const rowUpdates = useMemo<RowUpdates>(
    () => ({ available, noCompatible, updating, onUpdate, instanceId, onChangeVersion }),
    [available, noCompatible, updating, onUpdate, instanceId, onChangeVersion],
  );
  const updatable = useMemo(() => new Set(available.keys()), [available]);

  // 押して確かめ直した失敗は、それより後に確認が成功するまで出す
  const recheckError =
    recheck.error && recheck.submittedAt >= updates.dataUpdatedAt ? toErrorPayload(recheck.error) : null;
  const queryError = updates.error && !updates.data ? toErrorPayload(updates.error) : null;

  const onCheck = () => {
    if (recheck.isPending) return;
    recheck.mutate(undefined, {
      onSuccess: (data) => {
        if (data.updates.length === 0) toast.show({ kind: "info", message: t("modrinth.upToDate") });
      },
    });
  };

  // ファイルのドロップ。受け口はウィンドウ全体に 1 つなので、開いているサブタブの種類で追加する
  useEffect(() => {
    const blocked = () => latest.current.locked || latest.current.dialogOpen || adding.current;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    onFilesDropped({
      enter: () => {
        if (!blocked()) setDragging(true);
      },
      leave: () => setDragging(false),
      drop: (paths) => {
        setDragging(false);
        if (blocked() || paths.length === 0) return;
        addFiles(latest.current.kind, paths, false);
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
  }, [addFiles]);

  const closeDialog = () => setDialogOpen(false);

  const confirm = () => {
    if (!dialog) return;
    // 確認の間に起動やインストールが始まったときは、何もせずに閉じる
    if (locked) {
      closeDialog();
      return;
    }
    if (dialog.type === "disable") {
      closeDialog();
      toggle(dialog.item);
    } else if (dialog.type === "remove") {
      if (remove.isPending) return;
      remove.mutate(
        { kind: dialog.item.kind, relPath: dialog.item.relPath },
        { onError: toast.showError, onSettled: closeDialog },
      );
    } else {
      closeDialog();
      addFiles(dialog.kind, dialog.paths, true);
    }
  };

  const busy = locked || setEnabled.isPending || remove.isPending || update.isPending || change.isPending;

  const updateControls: UpdateControls | undefined = canUpdate
    ? {
        rows: rowUpdates,
        updatable,
        count: available.size,
        checking: recheck.isPending || (updates.isFetching && !updates.data),
        onCheck,
        onUpdateAll: () => void runUpdate([...available.keys()]),
        disabled: busy,
        checkError: recheckError ?? queryError,
        failures,
        onDismissFailures: () => setFailures(null),
      }
    : undefined;

  return (
    <div className="relative flex h-full min-h-0 flex-col gap-4">
      {locked && <Notice tone="info">{t("content.busy")}</Notice>}
      <Tabs
        selectedKey={kind}
        onSelectionChange={(key) => setKind(key as ContentKind)}
        className="flex min-h-0 flex-1 flex-col gap-4"
      >
        <TabList aria-label={t("content.tabs.label")} className="flex gap-1">
          {kinds.map((k) => (
            <Tab key={k} id={k} className={subTabClass}>
              <span>{t(`content.tabs.${k}`)}</span>
              {lists[k].data && <span className="tabular text-xs text-text-muted">{lists[k].data.length}</span>}
            </Tab>
          ))}
        </TabList>
        {kinds.map((k) => (
          <TabPanel key={k} id={k} className="flex min-h-0 flex-1 outline-none">
            <KindPanel
              kind={k}
              query={lists[k]}
              filter={filters[k]}
              onFilterChange={(next) => setFilters((f) => ({ ...f, [k]: next }))}
              onToggle={onToggle}
              onRemove={onRemove}
              busy={busy}
              // ローダーの無いインスタンスには MOD を入れられないので出さない
              addHref={k === "mod" && canUpdate ? `/instances/${instance.id}/modrinth` : undefined}
              updates={k === "mod" ? updateControls : undefined}
            />
          </TabPanel>
        ))}
      </Tabs>
      {dragging && (
        // 受け皿。ドラッグの間だけ、タブ全体を覆って出す
        <div className="pointer-events-none absolute -inset-3 z-10 flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-accent bg-surface-0/85 backdrop-blur-sm">
          <IconDrop aria-hidden className="size-8 text-accent" />
          <p className="font-display text-lg font-semibold text-text-strong">{t("content.drop.target")}</p>
          <p className="text-sm text-text-muted">{t("content.drop.accepts", { ext: extensions[kind] })}</p>
        </div>
      )}
      {dialog && (
        <ContentDialog
          dialog={dialog}
          isOpen={dialogOpen}
          onOpenChange={setDialogOpen}
          onConfirm={confirm}
          isPending={dialog.type === "remove" && remove.isPending}
        />
      )}
    </div>
  );
}

function rejectText(t: ReturnType<typeof useTranslation>["t"], reason: RejectReason, kind: ContentKind): string {
  if (reason === "wrongExtension") return t("content.reject.wrongExtension", { ext: extensions[kind] });
  return t(`content.reject.${reason}`);
}

/** 1 つの種類の、絞り込みと一覧。 */
function KindPanel(props: {
  kind: ContentKind;
  query: ReturnType<typeof useContentList>;
  filter: FilterState;
  onFilterChange: (next: FilterState) => void;
  onToggle: (item: ContentItem) => void;
  onRemove: (item: ContentItem) => void;
  busy: boolean;
  /** 「Modrinth から追加」の行き先。無ければボタンを出さない */
  addHref?: string;
  /** 更新の部品（MOD のサブタブだけ）。無ければ更新の確認もボタンも出さない */
  updates?: UpdateControls;
}) {
  const { t } = useTranslation();
  const { kind, query, filter, updates } = props;
  const categories = useCategories(kind);
  const [showExcluded, setShowExcluded] = useState(false);
  const items = query.data;
  // 並びは画面に出す名前の順にする（DB はパスの順で返す）
  const sorted = useMemo(() => sortByName(items ?? []), [items]);
  const updatable = updates?.updatable;
  const { visible, excluded } = useMemo(() => applyFilter(sorted, filter, updatable), [sorted, filter, updatable]);
  const metadata = isMetadataFiltered(filter);
  // メタデータの絞り込みを外したら、対象外の行を閉じた状態に戻す（付け直したときにいきなり開かない）
  useEffect(() => {
    if (!metadata) setShowExcluded(false);
  }, [metadata]);

  // 操作の直後の取り直しの失敗では、手元の一覧を使い続ける。何も無いときだけ失敗を出す
  if (!items && query.error)
    return (
      <div role="alert">
        <FailureNotice error={toErrorPayload(query.error)} />
      </div>
    );
  if (!items) return null;

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[13rem_minmax(0,1fr)] gap-6">
      {/* 項目の絞り込みは左の列に置く（Modrinth の検索画面と同じ並び）。長いときはこの列の中でスクロールする */}
      <div data-testid="content-facets" className="scroll-thin min-h-0 overflow-auto pr-1">
        <ContentFilter
          state={filter}
          onChange={props.onFilterChange}
          categories={categories.data ?? null}
          showStatus
          showSearch={false}
          statuses={updates ? modStatuses : undefined}
        />
      </div>
      <div className="flex min-h-0 flex-col gap-2">
        {/* 一覧の上の行。名前の入力と、MOD のサブタブでは「Modrinth から追加」を置く */}
        <div className="flex shrink-0 items-center gap-2 pb-1">
          <ContentSearchField
            value={filter.query}
            onChange={(query) => props.onFilterChange({ ...filter, query })}
            className="min-w-0 flex-1"
          />
          {props.addHref !== undefined && (
            <Link to={props.addHref} className={addLink}>
              <IconPlus aria-hidden className="size-4 shrink-0" />
              <span>{t("modrinth.addFromModrinth")}</span>
            </Link>
          )}
        </div>
        {updates && <UpdateBar updates={updates} />}
        {updates?.failures && updates.failures.length > 0 && (
          <UpdateFailures failures={updates.failures} onDismiss={updates.onDismissFailures} />
        )}
        {metadata && excluded.length > 0 && (
          <div className="flex items-center gap-3 px-3 text-sm text-text-muted">
            <span>{t("content.excluded", { count: excluded.length })}</span>
            <Button onPress={() => setShowExcluded((v) => !v)} aria-expanded={showExcluded} className={textButton}>
              {showExcluded ? t("content.excludedHide") : t("content.excludedShow")}
            </Button>
          </div>
        )}
        {items.length === 0 ? (
          <p className="px-3 py-2 text-sm text-text-muted">{t(`content.empty.${kind}`)}</p>
        ) : visible.length === 0 ? (
          <p className="px-3 py-2 text-sm text-text-muted">{t("content.noMatch")}</p>
        ) : (
          <div className="min-h-0 flex-1">
            <ContentList
              items={visible}
              onToggle={props.onToggle}
              onRemove={props.onRemove}
              busy={props.busy}
              updates={updates?.rows}
            />
          </div>
        )}
        {metadata && showExcluded && excluded.length > 0 && (
          <div
            className="shrink-0 border-t border-line pt-2"
            style={{ height: Math.min(excluded.length, EXCLUDED_ROWS) * ROW_SIZE + 9 }}
          >
            <ContentList
              items={excluded}
              onToggle={props.onToggle}
              onRemove={props.onRemove}
              busy={props.busy}
              updates={updates?.rows}
              label={t("content.excludedList")}
            />
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 一覧の上の、更新の行。右に「更新を確認」と、更新があるときだけ「すべて更新（件数）」を置く。
 * 確認の失敗のうちオフラインと 429 は、左に「あとで確認します」とだけ控えめに出す。それ以外は失敗として示す。
 */
function UpdateBar({ updates }: { updates: UpdateControls }) {
  const { t } = useTranslation();
  const error = updates.checkError;
  const quiet = error !== null && isQuietCheckError(error);
  return (
    <>
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2">
        <p role="status" className="flex min-w-0 flex-1 items-center gap-2 px-3 text-sm text-text-muted">
          {quiet && (
            <>
              <IconOffline aria-hidden className="size-4 shrink-0" />
              <span>{t("modrinth.checkLater")}</span>
            </>
          )}
        </p>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            onPress={updates.onCheck}
            isDisabled={updates.checking || updates.disabled}
            className={barButton}
          >
            <IconCheck aria-hidden className="size-4 shrink-0" />
            <span>{updates.checking ? t("modrinth.checking") : t("modrinth.checkUpdates")}</span>
          </Button>
          {updates.count > 0 && (
            <Button onPress={updates.onUpdateAll} isDisabled={updates.disabled} className={barButton}>
              <IconUpdate aria-hidden className="size-4 shrink-0 text-accent" />
              <span>{t("modrinth.updateAll", { count: updates.count })}</span>
            </Button>
          )}
        </div>
      </div>
      {error !== null && !quiet && (
        <div role="alert" className="px-3">
          <FailureNotice error={error} />
        </div>
      )}
    </>
  );
}

/** 更新で失敗した MOD の名前と理由。更新が終わったあとに、閉じるまで一覧の上に出す。 */
function UpdateFailures({ failures, onDismiss }: { failures: ItemFailure[]; onDismiss: () => void }) {
  const { t } = useTranslation();
  const titleId = useId();
  return (
    <section
      role="alert"
      aria-labelledby={titleId}
      className="flex shrink-0 flex-col gap-2 rounded-md border border-line bg-surface-1 px-3 py-2.5"
    >
      <div className="flex items-center justify-between gap-3">
        <p id={titleId} className="text-sm font-semibold text-text-strong">
          {t("modrinth.updateFailures", { count: failures.length })}
        </p>
        <Button onPress={onDismiss} className={textButton}>
          {t("modrinth.dismiss")}
        </Button>
      </div>
      <ul className="scroll-thin flex max-h-40 flex-col gap-2 overflow-auto">
        {failures.map((f, i) => (
          <li key={`${i}-${f.name}`} className="flex flex-col gap-1">
            <span className="select-text text-sm text-text-strong">{f.name}</span>
            <FailureNotice error={f.error} />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** 依存の警告、削除、上書きの確認。 */
function ContentDialog(props: {
  dialog: Dialog;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  isPending: boolean;
}) {
  const { t } = useTranslation();
  const { dialog } = props;
  const common = {
    isOpen: props.isOpen,
    onOpenChange: props.onOpenChange,
    onConfirm: props.onConfirm,
    isPending: props.isPending,
    cancelLabel: t("content.confirm.cancel"),
  };
  if (dialog.type === "disable")
    return (
      <ConfirmDialog
        {...common}
        tone="default"
        title={t("content.confirm.disableTitle", { name: displayName(dialog.item) })}
        confirmLabel={t("content.confirm.disable")}
      >
        <p>{t("content.confirm.disableBody")}</p>
        <NameList names={dialog.dependents.map(displayName)} />
      </ConfirmDialog>
    );
  if (dialog.type === "remove")
    return (
      <ConfirmDialog
        {...common}
        tone="danger"
        title={t("content.confirm.removeTitle", { name: displayName(dialog.item) })}
        confirmLabel={t("content.confirm.remove")}
      >
        <p>{t("content.confirm.removeBody")}</p>
        {dialog.dependents.length > 0 && (
          <>
            <p>{t("content.confirm.removeDependents")}</p>
            <NameList names={dialog.dependents.map(displayName)} />
          </>
        )}
      </ConfirmDialog>
    );
  return (
    <ConfirmDialog
      {...common}
      tone="default"
      title={t("content.confirm.conflictTitle")}
      confirmLabel={t("content.confirm.replace")}
    >
      <p>{t("content.confirm.conflictBody")}</p>
      <NameList names={dialog.names} />
    </ConfirmDialog>
  );
}

function NameList({ names }: { names: string[] }) {
  return (
    <ul className="scroll-thin max-h-40 list-disc overflow-auto pl-5 text-text-strong">
      {names.map((name, i) => (
        <li key={`${i}-${name}`} className="select-text">
          {name}
        </li>
      ))}
    </ul>
  );
}
