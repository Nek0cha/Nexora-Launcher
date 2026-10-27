import { type ReactNode, useEffect, useId, useState } from "react";
import {
  Button,
  Checkbox,
  Dialog,
  DialogTrigger,
  Disclosure,
  DisclosurePanel,
  Heading,
  Input,
  Label,
  Modal,
  ModalOverlay,
  Tab,
  TabList,
  TabPanel,
  Tabs,
  TextField,
} from "react-aria-components";
import type { UseQueryResult } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconCaretRight from "~icons/ph/caret-right";
import IconPlus from "~icons/ph/plus";
import IconRetry from "~icons/ph/arrow-clockwise";
import { toErrorPayload } from "../api/ipc";
import type { LoaderKind } from "../bindings/LoaderKind";
import {
  fallbackLoader,
  LOADER_TABS,
  pickLoaderVersion,
  type TabState,
  tabState,
  versionForLoader,
  versionsFor,
} from "../features/instances/createLoader";
import { useSyncProfiles } from "../features/sync/useSyncProfiles";
import { defaultName, nextName } from "../features/instances/createName";
import { isValid, MEMORY_DEFAULT_MB, type SettingsForm, toSettings } from "../features/instances/settingsForm";
import {
  useCreateInstance,
  useLoaderGameVersions,
  useLoaderVersions,
  useVersions,
} from "../features/instances/useCreateInstance";
import { FailureNotice } from "./FailureNotice";
import { InstanceAdvancedFields } from "./InstanceAdvancedFields";
import { modalClass, overlayClass } from "./modal";
import { Select } from "./Select";
import { controlWidth, inputBase } from "./settingsLayout";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 空の状態の主な操作。プレイボタンと同じ明暗の反転の塗りと、押下の縮み
const primaryTrigger = [
  "play-press inline-flex h-11 items-center gap-2 rounded-lg bg-text-strong px-5",
  "font-display font-semibold text-surface-0 data-hovered:opacity-90",
  ring,
].join(" ");

// 見出しの横に置く控えめな操作。面は乗せたときだけ現れる
const quietTrigger = [
  "inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3",
  ring,
].join(" ");

const submitButton = [
  "inline-flex h-9 items-center gap-2 rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

// ローダーのタブ。インスタンスの詳細のタブと同じ下線の形。選べないタブは文字を薄くする
const tabClass = [
  "-mb-px cursor-default border-b-2 border-transparent px-1 pb-2 text-sm text-text-muted outline-none",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:text-text selected:border-text-strong selected:text-text-strong",
  "disabled:text-text-faint disabled:hover:text-text-faint",
  "focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

// 詳細設定の開閉のボタン。行の全幅を押せるようにし、面は乗せたときだけ現れる
const disclosureButton = [
  "flex h-11 w-full items-center gap-2 px-5 text-left text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 hover:text-text",
  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent",
].join(" ");

// 名前の欄は、Select と同じ「ラベルが左、入力が右」の行にし、右端を揃える。
// 右の列の幅（controlWidth）は、名前の欄、バージョンの選択欄、読み込み中の表示で揃える
const inputClass = `${inputBase} ${controlWidth}`;

/** 詳細設定の初期値。閉じたまま作ると、従来と同じ自動の設定になる。 */
const ADVANCED_DEFAULTS: Omit<SettingsForm, "name"> = {
  memoryMode: "auto",
  memoryMb: MEMORY_DEFAULT_MB,
  javaMode: "auto",
  javaPath: "",
  jvmArgs: "",
};

export function CreateInstanceDialog({ trigger }: { trigger: "primary" | "quiet" }) {
  const { t } = useTranslation();
  return (
    <DialogTrigger>
      <Button className={trigger === "primary" ? primaryTrigger : quietTrigger}>
        <IconPlus aria-hidden className={trigger === "primary" ? "size-5" : "size-4"} />
        <span>{t("create.open")}</span>
      </Button>
      <ModalOverlay isDismissable className={overlayClass}>
        <Modal className={modalClass}>
          <Dialog className="outline-hidden">{({ close }) => <CreateForm onDone={close} />}</Dialog>
        </Modal>
      </ModalOverlay>
    </DialogTrigger>
  );
}

/** ダイアログの中身。開くたびに作り直されるため、入力は毎回まっさらから始まる。 */
function CreateForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const reasonId = useId();
  const versions = useVersions(true);
  // 各ローダーの対応バージョンは、開いたときに取りに行く（キャッシュされる）
  const fabricGames = useLoaderGameVersions("fabric", true);
  const forgeGames = useLoaderGameVersions("forge", true);
  const neoforgeGames = useLoaderGameVersions("neoforge", true);
  const quiltGames = useLoaderGameVersions("quilt", true);
  const create = useCreateInstance();
  const profiles = useSyncProfiles().data ?? [];
  const [useShared, setUseShared] = useState(false);
  const [chosenProfile, setChosenProfile] = useState<string | null>(null);
  // 選んだ共通設定が消えていたら、先頭にする
  const profileId = profiles.find((p) => p.id === chosenProfile)?.id ?? profiles[0]?.id ?? null;
  const [loader, setLoader] = useState<LoaderKind>("vanilla");
  const [version, setVersion] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [chosenLoaderVersion, setChosenLoaderVersion] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(ADVANCED_DEFAULTS);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const vanillaIds = versions.data?.map((v) => v.id) ?? [];
  const gameVersions: Partial<Record<LoaderKind, string[]>> = {
    fabric: fabricGames.data,
    forge: forgeGames.data,
    neoforge: neoforgeGames.data,
    quilt: quiltGames.data,
  };
  const gameQueriesByLoader = { fabric: fabricGames, forge: forgeGames, neoforge: neoforgeGames, quilt: quiltGames };
  const gameQueries = Object.values(gameQueriesByLoader);
  const failedGames = gameQueries.find((q) => q.error);
  const loaderVersions = useLoaderVersions(loader, version, loader !== "vanilla");

  const nameFor = (l: LoaderKind, v: string) => defaultName(l, v, t(`create.tab.${l}`));
  const currentDefault = version === null ? null : nameFor(loader, version);

  // ローダーとバージョンを一緒に切り替える。名前は、利用者が書き換えていなければ新しい既定の名前にする
  const select = (nextLoader: LoaderKind, nextVersion: string) => {
    const nextDefault = nameFor(nextLoader, nextVersion);
    setName((current) => nextName(current, currentDefault, nextDefault));
    setLoader(nextLoader);
    setVersion(nextVersion);
  };

  // 一覧が届いたら、最初の項目（最新の正式版）を選ぶ
  const first = versions.data?.[0]?.id;
  useEffect(() => {
    if (version !== null || first === undefined) return;
    setVersion(first);
    setName((current) => nextName(current, null, first));
  }, [first, version]);

  // 対応バージョンの一覧が取り直され、選んでいるローダーが今のバージョンに対応しなくなったら Vanilla に戻す
  const fallback = version === null ? loader : fallbackLoader(loader, version, vanillaIds, gameVersions);
  useEffect(() => {
    if (version !== null && fallback !== loader) select(fallback, version);
    // select は毎回作り直されるが、判定に使う値はこの 3 つだけである
  }, [fallback, loader, version]);

  const tabInfo = (l: LoaderKind): TabState | { enabled: false; reason: null } => {
    if (l !== "vanilla") {
      // 取得に失敗した一覧は「読み込み中」のままにしない。理由はダイアログの下部に出す
      if (gameQueriesByLoader[l].error) return { enabled: false, reason: null };
      // バージョンの一覧がまだ届いていない間は、ローダーの一覧も揃っていないものとして扱う
      if (version === null) return { enabled: false, reason: "loading" };
    }
    return tabState(l, vanillaIds, gameVersions);
  };
  const tabTitle = (s: ReturnType<typeof tabInfo>) => {
    if (s.enabled || s.reason === null) return undefined;
    return { noVersions: t("create.tabNoVersions"), loading: t("create.loadingLoaderVersions") }[s.reason];
  };

  const loaderVersion = loader === "vanilla" ? null : pickLoaderVersion(loaderVersions.data ?? [], chosenLoaderVersion);
  // 名前が空欄なら既定の名前で作る。検証も、送る値で行う
  const submitName = name.trim() || currentDefault || "";
  // バージョンが決まる前（名前の既定値が無い間）は、詳細設定の入力の不正とはみなさない
  const advancedInvalid = submitName !== "" && !isValid({ ...advanced, name: submitName });
  const ready = version !== null && (loader === "vanilla" || loaderVersion !== null) && !advancedInvalid;

  // 詳細設定の入力が不正なら、閉じていても開いて見せる
  useEffect(() => {
    if (advancedInvalid && !advancedOpen) setAdvancedOpen(true);
  }, [advancedInvalid, advancedOpen]);

  const submit = () => {
    if (!ready || version === null || create.isPending) return;
    const s = toSettings({ ...advanced, name: submitName });
    create.mutate(
      {
        request: {
          name: s.name,
          mcVersion: version,
          loader,
          loaderVersion,
          memoryMode: s.memoryMode,
          memoryMb: s.memoryMb,
          javaMode: s.javaMode,
          javaPath: s.javaPath,
          jvmArgs: s.jvmArgs,
        },
        syncProfileId: useShared ? profileId : null,
      },
      { onSuccess: onDone },
    );
  };

  const versionOptions = versionsFor(loader, vanillaIds, gameVersions[loader]);

  return (
    // 詳細設定を開くと画面の高さを超えうるため、見出しと下部のボタンは固定し、間だけを縦に送る。
    // 高さの上限は、暗幕の上の余白（15vh）と下の余白（1rem）を除いた分
    <form
      className="flex max-h-[calc(85vh-1rem)] flex-col"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <Heading slot="title" className="px-5 pt-5 font-display text-lg font-semibold text-text-strong">
        {t("create.title")}
      </Heading>
      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto">
        <Tabs
          selectedKey={loader}
          onSelectionChange={(key) => {
            if (version === null || key === loader) return;
            // 今のバージョンに対応していなければ、そのローダーが対応する最新のバージョンへ移る
            const next = versionForLoader(key as LoaderKind, version, vanillaIds, gameVersions);
            if (next !== null) select(key as LoaderKind, next);
          }}
          className="px-5 pt-4"
        >
          <TabList aria-label={t("create.loader")} className="flex gap-5 border-b border-line">
            {LOADER_TABS.map((l) => {
              const state = tabInfo(l);
              return (
                <Tab
                  key={l}
                  id={l}
                  isDisabled={!state.enabled}
                  aria-describedby={tabTitle(state) === undefined ? undefined : `${reasonId}-${l}`}
                  className={tabClass}
                >
                  {/* Tab は title を受け取らないため、選べない理由は中の文字に付ける */}
                  <span title={tabTitle(state)}>{t(`create.tab.${l}`)}</span>
                </Tab>
              );
            })}
          </TabList>
          {/* 選べないタブの理由。title は支援技術に届きにくいため、見えないテキストとして aria-describedby から参照する */}
          {LOADER_TABS.map((l) => {
            const reason = tabTitle(tabInfo(l));
            return reason === undefined ? null : (
              <span key={l} id={`${reasonId}-${l}`} className="sr-only">
                {reason}
              </span>
            );
          })}
          {/* 中身はどのタブでも同じ欄なので、選んでいるタブの id を持つ 1 つのパネルを使い回す */}
          <TabPanel id={loader} className="flex flex-col gap-3 pt-4 pb-5 outline-none">
            <TextField value={name} onChange={setName} maxLength={64} autoFocus className="flex items-center justify-between gap-4">
              <Label className="text-text">{t("create.name")}</Label>
              {/* 空欄のまま作ると、既定の名前を使う。そのことを入力例として見せる */}
              <Input placeholder={currentDefault ?? undefined} className={inputClass} />
            </TextField>
            <QueryRow label={t("create.version")} query={versions} loadingText={t("create.loadingVersions")} ready={version !== null}>
              <Select
                label={t("create.version")}
                value={version ?? ""}
                options={versionOptions.map((id) => ({ id, label: id }))}
                onChange={(next) => select(fallbackLoader(loader, next, vanillaIds, gameVersions), next)}
                triggerClassName={controlWidth}
              />
            </QueryRow>
            {loader !== "vanilla" && (
              <QueryRow
                label={t("create.loaderVersion")}
                query={loaderVersions}
                loadingText={t("create.loadingLoaderVersions")}
                emptyText={t("create.noLoaderVersions")}
                ready={loaderVersion !== null}
              >
                <Select
                  label={t("create.loaderVersion")}
                  value={loaderVersion ?? ""}
                  options={(loaderVersions.data ?? []).map((e) => ({ id: e.version, label: e.version }))}
                  onChange={setChosenLoaderVersion}
                  triggerClassName={controlWidth}
                />
              </QueryRow>
            )}
          </TabPanel>
        </Tabs>
        {profileId !== null && (
          <div className="flex flex-col gap-3 border-t border-line px-5 py-3">
            <Checkbox isSelected={useShared} onChange={setUseShared} className={`group flex w-fit items-center gap-2 text-text ${ring}`}>
              <span
                aria-hidden
                className="flex size-4 items-center justify-center rounded-sm border border-line-control bg-surface-1 text-surface-0 group-data-selected:border-text-strong group-data-selected:bg-text-strong"
              >
                <IconCheck className="size-3 opacity-0 group-data-selected:opacity-100" />
              </span>
              {t("create.shared.use")}
            </Checkbox>
            {useShared && profiles.length > 1 && (
              <div className="flex items-center justify-between gap-4">
                <span className="text-text">{t("create.shared.profile")}</span>
                <Select
                  label={t("create.shared.profile")}
                  value={profileId}
                  options={profiles.map((p) => ({ id: p.id, label: p.name }))}
                  onChange={setChosenProfile}
                  triggerClassName={controlWidth}
                />
              </div>
            )}
          </div>
        )}
        <Disclosure isExpanded={advancedOpen} onExpandedChange={setAdvancedOpen} className="group border-t border-line">
          <h3>
            <Button slot="trigger" className={disclosureButton}>
              <IconCaretRight
                aria-hidden
                className="size-4 transition-transform duration-(--duration-fast) ease-out-quint group-data-expanded:rotate-90 motion-reduce:transition-none"
              />
              <span>{t("create.advanced")}</span>
            </Button>
          </h3>
          {/* 閉じている間も中身は残すため、入力は失われない。高さが残らないよう、余白は中の要素に付ける */}
          <DisclosurePanel>
            <div className="px-5 pb-3">
              <InstanceAdvancedFields form={{ ...advanced, name }} onChange={(patch) => setAdvanced((f) => ({ ...f, ...patch }))} />
            </div>
          </DisclosurePanel>
        </Disclosure>
        {(failedGames || create.error) && (
          <div className="flex flex-col gap-3 border-t border-line px-5 py-4">
            {failedGames?.error && (
              <div role="alert" className="flex flex-col items-start gap-2">
                <FailureNotice error={toErrorPayload(failedGames.error)} />
                <Button
                  onPress={() => gameQueries.filter((q) => q.error).forEach((q) => void q.refetch())}
                  isPending={gameQueries.some((q) => q.isFetching)}
                  className={secondaryButton}
                >
                  <IconRetry aria-hidden className="size-4" />
                  <span>{t("create.retryLoaders")}</span>
                </Button>
              </div>
            )}
            {create.error && (
              <div role="alert">
                <FailureNotice error={toErrorPayload(create.error)} />
              </div>
            )}
          </div>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-5 py-3">
        <Button slot="close" className={secondaryButton}>
          {t("create.cancel")}
        </Button>
        <Button type="submit" isDisabled={!ready} isPending={create.isPending} className={submitButton}>
          {t("create.submit")}
        </Button>
      </div>
    </form>
  );
}

/**
 * 一覧を取りに行く選択欄の行。失敗なら再試行、読み込み中なら同じ形の表示、届いたら children を出す。
 * emptyText を渡すと、取得に成功して一覧が空だったときに、読み込み中の代わりにそれを出す。
 */
function QueryRow({
  label,
  query,
  loadingText,
  emptyText,
  ready,
  children,
}: {
  label: string;
  query: Pick<UseQueryResult<unknown[]>, "data" | "error" | "isFetching" | "isSuccess" | "refetch">;
  loadingText: string;
  emptyText?: string;
  ready: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  if (query.error) {
    // 行の形は残し、右の列に再試行を置く。何が失敗したかは行の下に出す
    return (
      <div role="alert" className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-4">
          <span className="text-text">{label}</span>
          <Button onPress={() => void query.refetch()} isPending={query.isFetching} className={`${secondaryButton} ${controlWidth} justify-center`}>
            <IconRetry aria-hidden className="size-4" />
            <span>{t("create.retry")}</span>
          </Button>
        </div>
        <FailureNotice error={toErrorPayload(query.error)} />
      </div>
    );
  }
  if (!ready && emptyText !== undefined && query.isSuccess && !query.isFetching && query.data?.length === 0) {
    // 取得はできたが選べるものが無い。読み込み中と見分けられるよう、理由を同じ行の形で出す。
    // 文が長いため、高さは折り返しに合わせて伸ばす
    return (
      <div className="flex items-center justify-between gap-4">
        <span className="text-text">{label}</span>
        <span
          role="status"
          className={`flex min-h-9 ${controlWidth} items-center rounded-md border border-line bg-surface-1 px-3 py-1.5 text-sm text-text-muted`}
        >
          {emptyText}
        </span>
      </div>
    );
  }
  if (!ready) {
    // 届いた後の Select と同じ行の形と高さにして、置き換わってもレイアウトを動かさない
    return (
      <div aria-busy="true" className="flex items-center justify-between gap-4">
        <span className="text-text">{label}</span>
        <span className={`flex h-9 ${controlWidth} items-center rounded-md border border-line bg-surface-1 px-3 text-sm text-text-muted`}>
          {loadingText}
        </span>
      </div>
    );
  }
  return children;
}
