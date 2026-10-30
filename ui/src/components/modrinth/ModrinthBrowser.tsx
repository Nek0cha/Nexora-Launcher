import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";
import {
  Button,
  Label,
  ListBox,
  ListBoxItem,
  Popover,
  Select as AriaSelect,
  type Key,
  SelectValue,
  ToggleButton,
  ToggleButtonGroup,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCaret from "~icons/ph/caret-up-down";
import IconCheck from "~icons/ph/check";
import IconLock from "~icons/ph/lock-simple";
import IconPlus from "~icons/ph/plus";
import IconOffline from "~icons/ph/wifi-slash";
import { openModrinthProject } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { InstallPreview } from "../../bindings/InstallPreview";
import type { Instance } from "../../bindings/Instance";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { SearchHit } from "../../bindings/SearchHit";
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { SearchSort } from "../../bindings/SearchSort";
import { emptyFilter, type FilterState } from "../../features/content/contentFilter";
import { useCategories, useContentList } from "../../features/content/useContent";
import { useVersions } from "../../features/instances/useCreateInstance";
import { useInstances } from "../../features/instances/useInstances";
import { loaderSlugs } from "../../features/modrinth/compatibleInstances";
import { useDebouncedValue } from "../../features/modrinth/useDebounced";
import { useInstallContent, useModrinthSearch, usePreviewInstall } from "../../features/modrinth/useModrinth";
import { SETTINGS_KEY, useSettings } from "../../features/settings/useSettings";
import { ConfirmDialog } from "../ConfirmDialog";
import { ChipGroup, ChipSkeleton, ContentFilter, ContentSearchField, FilterSkeleton } from "../content/ContentFilter";
import { FailureNotice } from "../FailureNotice";
import { ReadyGate, useDelayedFlag, useLatchedPending } from "../ReadyGate";
import { Skeleton } from "../Skeleton";
import { useToast } from "../toast/useToast";
import { AddToInstanceMenu } from "./AddToInstanceMenu";
import { StableLabel } from "../StableLabel";
import { actionButton, HitCard } from "./HitCard";

/** 追加ボタンの文字（追加、追加中、導入済み）。幅をそろえるために、すべてを重ねて置く */
const installLabels = (t: (key: "modrinth.install" | "modrinth.installing" | "modrinth.installed") => string) => [
  t("modrinth.install"),
  t("modrinth.installing"),
  t("modrinth.installed"),
];

/** 検索の入力が止まってから送るまでの時間（ミリ秒） */
export const SEARCH_DEBOUNCE_MS = 300;

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 全体の画面で選べるローダー（Modrinth のローダー名と同じ）
type ModLoader = Exclude<LoaderKind, "vanilla">;
const loaderOptions: ModLoader[] = ["fabric", "quilt", "forge", "neoforge"];

const ANY_VERSION = "any";

// 本文の中の控えめな文字のボタン（失敗の説明の「詳細」と同じ形）
const textButton = [
  "rounded-sm text-sm text-text-muted underline underline-offset-2",
  "data-hovered:text-text data-pressed:text-text-strong",
  ring,
].join(" ");

// 絞り込みの列の選択。ラベルは札の群と同じく上に置く（Select の一覧の見た目と動きは共通の部品と同じ）
const triggerClass = [
  "flex h-9 w-full items-center justify-between gap-2 rounded-md border border-line-control bg-surface-1 px-3 text-left text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  ring,
].join(" ");

const popoverMotion = [
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const sortOptions: SearchSort[] = ["relevance", "downloads"];

// 並び順の切り替え。2 つだけなので、枠の中に並べて選んだ方に面を敷く（サブタブと同じ見せ方）
const sortItemClass = [
  "inline-flex h-7 cursor-default items-center rounded-sm px-2.5 text-xs text-text-muted outline-none",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:text-text selected:bg-surface-3 selected:text-text-strong",
  ring,
].join(" ");

/** 並び順の選択。選んでいる方を押しても外れない。 */
function SortToggle(props: { value: SearchSort; onChange: (value: SearchSort) => void }) {
  const { t } = useTranslation();
  return (
    <ToggleButtonGroup
      aria-label={t("modrinth.sort")}
      selectionMode="single"
      disallowEmptySelection
      selectedKeys={[props.value]}
      onSelectionChange={(keys: Set<Key>) => {
        const [next] = keys;
        if (next !== undefined) props.onChange(next as SearchSort);
      }}
      className="inline-flex h-9 shrink-0 items-center gap-0.5 rounded-md border border-line-control p-0.5"
    >
      {sortOptions.map((id) => (
        <ToggleButton key={id} id={id} className={sortItemClass}>
          {t(`modrinth.sortValue.${id}`)}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  );
}

const optionClass =
  "flex h-9 cursor-default items-center gap-2 rounded-sm px-2 text-sm text-text outline-none hover:bg-surface-3 focused:bg-surface-3 selected:text-text-strong";

/** Minecraft のバージョンの選択（全体の画面だけ）。 */
function VersionSelect(props: { value: string | null; onChange: (value: string | null) => void }) {
  const { t } = useTranslation();
  const versions = useVersions(true);
  const versionsError = versions.error ? toErrorPayload(versions.error) : null;
  const options = [
    { id: ANY_VERSION, label: t("modrinth.anyVersion") },
    ...(versions.data ?? []).map((v) => ({ id: v.id, label: v.id })),
  ];
  return (
    <div className="flex flex-col gap-2">
      <AriaSelect
        value={props.value ?? ANY_VERSION}
        onChange={(key) => {
          if (key !== null) props.onChange(key === ANY_VERSION ? null : String(key));
        }}
        className="flex flex-col gap-2"
      >
        <Label className="text-xs text-text-muted">{t("modrinth.mcVersion")}</Label>
        <Button className={triggerClass}>
          <SelectValue className="tabular truncate">{({ selectedText }) => selectedText}</SelectValue>
          <IconCaret aria-hidden className="size-4 shrink-0 text-text-muted" />
        </Button>
        <Popover
          offset={4}
          className={`min-w-(--trigger-width) rounded-lg border border-line bg-surface-2 p-1 shadow-float ${popoverMotion}`}
        >
          <ListBox items={options} className="scroll-thin max-h-72 overflow-auto outline-none">
            {(o) => (
              <ListBoxItem id={o.id} textValue={o.label} className={optionClass}>
                {({ isSelected }) => (
                  <>
                    <span className="grid size-4 shrink-0 place-items-center">
                      {isSelected && <IconCheck aria-hidden className="size-4 text-accent" />}
                    </span>
                    <span className="tabular truncate">{o.label}</span>
                  </>
                )}
              </ListBoxItem>
            )}
          </ListBox>
        </Popover>
      </AriaSelect>
      {versionsError && (
        <div role="alert" data-testid="versions-failed" className="flex flex-col items-start gap-1">
          <FailureNotice error={versionsError} />
          <Button onPress={() => void versions.refetch()} className={textButton}>
            {t("modrinth.retry")}
          </Button>
        </div>
      )}
    </div>
  );
}

/** 並び順の切り替えと同じ枠。見えない文字で幅を決めるので、どの言語でも完成形と同じ幅になる */
function SortSkeleton() {
  const { t } = useTranslation();
  return (
    <div
      aria-hidden
      className="inline-flex h-9 shrink-0 items-center gap-0.5 rounded-md border border-transparent bg-surface-2 p-0.5 motion-safe:animate-pulse"
    >
      {sortOptions.map((id) => (
        <span key={id} className="inline-flex h-7 items-center rounded-sm px-2.5 text-xs">
          <span className="invisible">{t(`modrinth.sortValue.${id}`)}</span>
        </span>
      ))}
    </div>
  );
}

const SKELETON_HITS = 8;

/**
 * 検索画面と同じ並びと寸法のスケルトン。左の列（絞り込み）と、結果の行（アイコン、名前、説明、下の行）を
 * 完成形の数と大きさに近づけて置く。
 */
function BrowserSkeleton({ fixed }: { fixed: boolean }) {
  const { t } = useTranslation();
  return (
    <div data-testid="modrinth-skeleton" className="grid min-h-0 flex-1 grid-cols-[13rem_minmax(0,1fr)] gap-6">
      <span role="status" className="sr-only">
        {t("modrinth.searching")}
      </span>
      <div className="scroll-thin flex min-h-0 flex-col gap-4 overflow-hidden pr-1 [scrollbar-gutter:stable]">
        {fixed ? (
          <div className="flex flex-col gap-2">
            <Skeleton className="h-[18px] w-16" />
            <Skeleton className="h-5 w-32" />
          </div>
        ) : (
          <>
            <div className="flex flex-col gap-2">
              <Skeleton className="h-[18px] w-24" />
              <Skeleton className="h-9 w-full" />
            </div>
            <div className="flex flex-col gap-2">
              <Skeleton className="h-[18px] w-16" />
              <div className="flex flex-wrap gap-1.5">
                {loaderOptions.map((id) => (
                  <ChipSkeleton key={id} label={t(`create.tab.${id}`)} />
                ))}
              </div>
            </div>
          </>
        )}
        <FilterSkeleton status={false} />
      </div>
      <div className="flex min-h-0 flex-col gap-2 overflow-hidden">
        <div className="flex shrink-0 items-center gap-3 pb-1">
          <Skeleton className="h-9 min-w-0 flex-1" />
          <Skeleton className="h-9 w-32 shrink-0" />
          <SortSkeleton />
        </div>
        <div className="flex min-h-0 flex-1 flex-col divide-y divide-line overflow-hidden pr-1">
          {Array.from({ length: SKELETON_HITS }, (_, i) => (
            // 高さは実際の結果 1 件（HitCard の上下 py-4、アイコン 48px、説明 2 行、下の行）を幅 600px 前後で測った値
            <div key={i} className="grid h-[169.3px] shrink-0 grid-cols-[auto_minmax(0,1fr)] content-start gap-x-4 py-4">
              <Skeleton className="size-12" />
              <div className="flex min-w-0 flex-col gap-2">
                <Skeleton className="h-5 w-48 max-w-full" />
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-3/4" />
              </div>
              <div className="col-start-2 mt-2.5 flex items-center gap-3">
                <Skeleton className="h-4 w-20" />
                <Skeleton className="ml-auto h-8 w-24" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

type Confirming = { hit: SearchHit; target: Instance; preview: InstallPreview };

/** 追加の流れ。プレビュー → 非互換があれば確認 → 導入 → 結果をトーストで知らせる。 */
function useAddFlow() {
  const { t } = useTranslation();
  const toast = useToast();
  const client = useQueryClient();
  const preview = usePreviewInstall();
  const install = useInstallContent();
  // 追加中の結果（projectId）。同じ結果を続けて押しても 1 回にする
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const [confirming, setConfirming] = useState<Confirming | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);

  const mark = useCallback((projectId: string, on: boolean) => {
    setPending((prev) => {
      const next = new Set(prev);
      if (on) next.add(projectId);
      else next.delete(projectId);
      return next;
    });
  }, []);

  const runInstall = useCallback(
    async (hit: SearchHit, target: Instance, acceptIncompatible: boolean, enables: boolean) => {
      mark(hit.projectId, true);
      try {
        const outcome = await install.mutateAsync({
          instanceId: target.id,
          projectId: hit.projectId,
          acceptIncompatible,
        });
        if (outcome.state === "succeeded") {
          // 無効で入っていた同じ MOD を有効にしただけのときは、そう知らせる
          const key = enables ? "modrinth.enabled" : "modrinth.added";
          toast.show({ kind: "success", message: t(key, { name: hit.title, instance: target.name }) });
          const deps = outcome.report?.dependenciesAdded ?? 0;
          if (deps > 0) toast.show({ kind: "success", message: t("modrinth.dependenciesAdded", { count: deps }) });
          // 「最後に追加したインスタンス」が変わる
          void client.invalidateQueries({ queryKey: SETTINGS_KEY });
        } else if (outcome.state === "cancelled") {
          toast.show({ kind: "info", message: t("modrinth.cancelled", { name: hit.title }) });
        } else {
          toast.showError(outcome.error ?? new Error("install task failed"));
        }
      } catch (error) {
        toast.showError(error);
      } finally {
        mark(hit.projectId, false);
      }
    },
    [client, install.mutateAsync, mark, t, toast],
  );

  const add = useCallback(
    async (hit: SearchHit, target: Instance) => {
      if (pending.has(hit.projectId)) return;
      mark(hit.projectId, true);
      let result: InstallPreview;
      try {
        result = await preview.mutateAsync({ instanceId: target.id, projectId: hit.projectId });
      } catch (error) {
        toast.showError(error);
        mark(hit.projectId, false);
        return;
      }
      mark(hit.projectId, false);
      // 非互換の MOD が入っているときは、了承されるまで何も入れない
      if (result.incompatible.length > 0) {
        setConfirming({ hit, target, preview: result });
        setConfirmOpen(true);
        return;
      }
      await runInstall(hit, target, false, result.enablesInstalled);
    },
    [mark, pending, preview.mutateAsync, runInstall, toast],
  );

  const confirm = () => {
    if (!confirming) return;
    setConfirmOpen(false);
    void runInstall(confirming.hit, confirming.target, true, confirming.preview.enablesInstalled);
  };

  const dialog = confirming && (
    <ConfirmDialog
      isOpen={confirmOpen}
      onOpenChange={setConfirmOpen}
      onConfirm={confirm}
      isPending={false}
      tone="default"
      title={t("modrinth.confirmTitle", { name: confirming.hit.title })}
      confirmLabel={t("modrinth.installAnyway")}
      cancelLabel={t("modrinth.cancel")}
    >
      <p>{t("modrinth.incompatibleBody")}</p>
      <ul className="scroll-thin max-h-40 list-disc overflow-auto pl-5 text-text-strong">
        {confirming.preview.incompatible.map((name, i) => (
          <li key={`${i}-${name}`} className="select-text">
            {name}
          </li>
        ))}
      </ul>
      <p className="text-text-muted">
        {t("modrinth.confirmBody", {
          version: confirming.preview.versionNumber,
          count: confirming.preview.dependencyCount,
        })}
      </p>
    </ConfirmDialog>
  );

  return { add, pending, dialog };
}

/**
 * Modrinth の MOD の検索（spec 6.1）。`instance` があれば、その Minecraft のバージョンとローダーに固定し、
 * それらの絞り込みは出さずに「固定」と示す。なければ自由に選べ、結果ごとに追加先のインスタンスを選ぶ。
 */
export function ModrinthBrowser(props: { instance?: Instance }) {
  const { t, i18n } = useTranslation();
  const { instance } = props;
  const [filter, setFilter] = useState<FilterState>(emptyFilter);
  const [mcVersion, setMcVersion] = useState<string | null>(null);
  const [loaders, setLoaders] = useState<ModLoader[]>([]);
  const [sort, setSort] = useState<SearchSort>("relevance");
  const categories = useCategories("mod");
  const instances = useInstances();
  const settings = useSettings();
  const { add, pending, dialog } = useAddFlow();
  // インスタンスの画面では、有効で導入済みの MOD は「導入済み」にして追加を押せなくする
  const content = useContentList(instance?.id ?? "", "mod", instance !== undefined);
  const installed = useMemo(
    () =>
      new Set(
        (content.data ?? []).filter((c) => c.enabled && c.projectId !== null).map((c) => c.projectId as string),
      ),
    [content.data],
  );

  // 文字の入力だけ止まるのを待つ。札や選択は押したときにすぐ送る
  const text = useDebouncedValue(filter.query.trim(), SEARCH_DEBOUNCE_MS);
  const query = useMemo<SearchQuery>(
    () => ({
      query: text,
      mcVersion: instance ? instance.mcVersion : mcVersion,
      loaders: instance ? loaderSlugs(instance.loader) : loaders,
      categories: filter.categories,
      clientSides: filter.client,
      serverSides: filter.server,
      sort,
    }),
    [text, instance, mcVersion, loaders, filter.categories, filter.client, filter.server, sort],
  );
  const search = useModrinthSearch(instance?.id ?? null, query);
  const versions = useVersions(!instance);
  // 最初の表示は、絞り込みの材料（カテゴリ、バージョン、インスタンス、設定、導入済み）と最初の結果がそろってから。
  // 無効にしている問い合わせは待たない。そろったあとに pending へ戻っても枠には戻さない
  const gatePending = useLatchedPending(
    categories.isPending ||
      instances.isPending ||
      settings.isPending ||
      search.isPending ||
      (!instance && versions.isPending) ||
      (instance !== undefined && content.isPending),
  );

  // ページの境目で同じ結果が二度来ることがあるので、最初のものだけを残す
  const hits = useMemo(() => {
    const seen = new Set<string>();
    return (search.data?.pages ?? [])
      .flatMap((p) => p.hits)
      .filter((h) => (seen.has(h.projectId) ? false : (seen.add(h.projectId), true)));
  }, [search.data]);
  const total = search.data?.pages[0]?.total ?? null;
  const number = useMemo(() => new Intl.NumberFormat(i18n.language), [i18n.language]);

  const instanceList = instances.data ?? [];
  const lastAddedId = settings.data?.lastAddedInstance ?? null;
  const lastAdded = instanceList.find((i) => i.id === lastAddedId) ?? null;
  const toast = useToast();
  const open = (slug: string) => {
    openModrinthProject(slug).catch(toast.showError);
  };

  const error = search.error ? toErrorPayload(search.error) : null;
  // 新しい条件の結果を待っていて、いまは前の結果を見せている
  const refreshing = search.isPlaceholderData;
  // 薄くするのは待ちが長いときだけ。すぐ返るときに薄くして戻すと、フィルターを押すたびにちらつく
  const dimmed = useDelayedFlag(refreshing);

  return (
    <>
      <ReadyGate queries={[{ isPending: gatePending }]} skeleton={<BrowserSkeleton fixed={instance !== undefined} />}>
        <div className="grid min-h-0 flex-1 grid-cols-[13rem_minmax(0,1fr)] gap-6">
          {/* 絞り込みは左の列（コンテンツの一覧と同じ並び）。長いときはこの列の中でスクロールする */}
          <div data-testid="modrinth-facets" className="scroll-thin flex min-h-0 flex-col gap-4 overflow-auto pr-1 [scrollbar-gutter:stable]">
            {instance ? (
              <div className="flex flex-col gap-2">
                <span className="text-xs text-text-muted">{t("modrinth.fixedLabel")}</span>
                <p className="flex items-center gap-1.5 text-sm text-text">
                  <IconLock aria-hidden className="size-3.5 shrink-0 text-text-muted" />
                  <span>
                    {t("modrinth.fixed", { version: instance.mcVersion, loader: t(`create.tab.${instance.loader}`) })}
                  </span>
                </p>
              </div>
            ) : (
              <>
                <VersionSelect value={mcVersion} onChange={setMcVersion} />
                <ChipGroup
                  label={t("modrinth.loader")}
                  options={loaderOptions.map((id) => ({ id, label: t(`create.tab.${id}`) }))}
                  selected={loaders}
                  onChange={setLoaders}
                />
              </>
            )}
            <ContentFilter
              state={filter}
              onChange={setFilter}
              categories={categories.data ?? null}
              categoriesError={categories.error ? toErrorPayload(categories.error) : null}
              showStatus={false}
              showSearch={false}
            />
          </div>
          <div className="flex min-h-0 flex-col gap-2">
            <div className="flex shrink-0 items-center gap-3 pb-1">
              <ContentSearchField
                label={t("modrinth.search")}
                value={filter.query}
                onChange={(q) => setFilter((f) => ({ ...f, query: q }))}
                className="min-w-0 flex-1"
              />
              {/* 件数の場所は、決まる前から同じ幅で取る（出たときに検索欄が縮まない） */}
              <span data-testid="modrinth-total" className="tabular min-w-32 shrink-0 text-right text-sm text-text-muted">
                {total !== null && t("modrinth.total", { value: number.format(total) })}
              </span>
              <SortToggle value={sort} onChange={setSort} />
            </div>
            {/* 検索し直している間は、前の結果を残して薄くする（空にしたりスケルトンに戻したりしない） */}
            <div
              data-testid="modrinth-results"
              aria-busy={dimmed}
              className={`scroll-thin min-h-0 flex-1 overflow-auto pr-1 transition-opacity duration-(--duration-fast) ease-out-quint ${
                dimmed ? "opacity-60" : ""
              }`}
            >
              {search.isPending && !error ? (
                <p className="py-2 text-sm text-text-muted" role="status">
                  {t("modrinth.searching")}
                </p>
              ) : error && hits.length === 0 ? (
                <div role="alert" className="flex flex-col items-start gap-3 py-2">
                  {error.code === "net.offline" ? (
                    <p className="flex items-center gap-2 text-sm text-text">
                      <IconOffline aria-hidden className="size-4 shrink-0 text-text-muted" />
                      <span>{t("modrinth.offline")}</span>
                    </p>
                  ) : (
                    <FailureNotice error={error} />
                  )}
                  <Button onPress={() => void search.refetch()} className={textButton}>
                    {t("modrinth.retry")}
                  </Button>
                </div>
              ) : hits.length === 0 ? (
                <p className="py-2 text-sm text-text-muted">{t("modrinth.noResults")}</p>
              ) : (
                <>
                  <ul aria-label={t("modrinth.results")} className="@container flex flex-col divide-y divide-line">
                    {hits.map((hit) => (
                      <li key={hit.projectId}>
                        <HitCard
                          hit={hit}
                          onOpen={open}
                          showLoaders={!instance}
                          actions={
                            instance && installed.has(hit.projectId) ? (
                              <Button isDisabled className={actionButton}>
                                <IconCheck aria-hidden className="size-3.5 shrink-0" />
                                <StableLabel labels={installLabels(t)} active={2} />
                              </Button>
                            ) : instance ? (
                              <Button
                                isDisabled={pending.has(hit.projectId)}
                                onPress={() => void add(hit, instance)}
                                className={actionButton}
                              >
                                <IconPlus aria-hidden className="size-3.5 shrink-0" />
                                <StableLabel labels={installLabels(t)} active={pending.has(hit.projectId) ? 1 : 0} />
                              </Button>
                            ) : (
                              <AddToInstanceMenu
                                hit={hit}
                                instances={instanceList}
                                lastAdded={lastAdded}
                                pending={pending.has(hit.projectId)}
                                onAdd={(target) => void add(hit, target)}
                              />
                            )
                          }
                        />
                      </li>
                    ))}
                  </ul>
                  {search.isFetchNextPageError && error && (
                    <div role="alert" className="py-2">
                      <FailureNotice error={error} />
                    </div>
                  )}
                  {search.hasNextPage && (
                    <div className="flex justify-center py-4">
                      <Button
                        onPress={() => void search.fetchNextPage()}
                        isPending={search.isFetchingNextPage}
                        isDisabled={refreshing}
                        className={actionButton}
                      >
                        {t("modrinth.loadMore")}
                      </Button>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </div>
      </ReadyGate>
      {dialog}
    </>
  );
}
