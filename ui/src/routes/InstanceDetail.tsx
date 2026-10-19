import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import {
  Button,
  Input,
  Label,
  Tab,
  TabList,
  TabPanel,
  Tabs,
  Text,
  TextField,
  Tooltip,
  TooltipTrigger,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router";
import IconBack from "~icons/ph/arrow-left";
import IconGame from "~icons/ph/folder-open";
import IconScreenshots from "~icons/ph/image";
import IconInfo from "~icons/ph/info";
import IconMods from "~icons/ph/puzzle-piece";
import IconWorlds from "~icons/ph/globe-hemisphere-west";
import IconWarning from "~icons/ph/warning";
import { openInstanceFolder, systemMemoryMb } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { FolderKind } from "../bindings/FolderKind";
import type { Instance } from "../bindings/Instance";
import type { JavaMode } from "../bindings/JavaMode";
import type { MemoryMode } from "../bindings/MemoryMode";
import { FailureNotice } from "../components/FailureNotice";
import { MemorySlider } from "../components/MemorySlider";
import { NumberField } from "../components/NumberField";
import { Select } from "../components/Select";
import { useToast } from "../components/toast/useToast";
import { tooltipClass } from "../components/tooltip";
import {
  hasMemoryFlag,
  isDirty,
  isValid,
  MEMORY_MAX_MB,
  MEMORY_MIN_MB,
  memoryError,
  type SettingsForm,
  toForm,
  toSettings,
} from "../features/instances/settingsForm";
import { SLIDER_STEP_MB, sliderMax } from "../features/instances/memorySlider";
import { useUpdateInstance } from "../features/instances/useInstanceActions";
import { useInstances } from "../features/instances/useInstances";
import { useRunning } from "../features/play/useGameSync";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 見出しの横の控えめなリンク。面は乗せたときだけ現れる（一覧の行の操作と同じ）
const quietLink = [
  "inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

// 右の列の幅。名前の欄と、自動／手動の選択欄で揃える
const controlWidth = "w-60";

const inputBase = [
  "h-9 rounded-md border border-line-control bg-surface-1 px-3 text-text",
  "placeholder:text-text-faint",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:border-text-muted",
  "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
].join(" ");

const saveButton = [
  "inline-flex h-9 items-center gap-2 rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

const tabClass = [
  "-mb-px cursor-default border-b-2 border-transparent px-1 pb-2 text-sm text-text-muted outline-none",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:text-text selected:border-text-strong selected:text-text-strong",
  "focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

// 詳細の見出しの横に並べるフォルダのボタン
const folderButtons: { kind: FolderKind; icon: typeof IconGame }[] = [
  { kind: "game", icon: IconGame },
  { kind: "mods", icon: IconMods },
  { kind: "screenshots", icon: IconScreenshots },
  { kind: "saves", icon: IconWorlds },
];

const iconButton = [
  "inline-flex size-8 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

const title = "font-display text-xl font-semibold tracking-[-0.01em] text-text-strong";

export default function InstanceDetail() {
  const { t } = useTranslation();
  const { id } = useParams();
  const { data, error } = useInstances();
  const running = useRunning();
  // フォルダを開けなかったときはトーストで出す
  const toast = useToast();

  // 取り直しの失敗では、手元の一覧を使い続ける
  if (!data && error)
    return (
      <div className="flex flex-col gap-8 p-10">
        <h1 className={title}>{t("nav.instances")}</h1>
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  if (!data) return null;

  const instance = data.find((i) => i.id === id);
  if (!instance)
    return (
      <div className="flex max-w-2xl flex-col items-start gap-4 p-10">
        <p className="text-text">{t("instances.detail.notFound")}</p>
        <Link to="/instances" className={`${quietLink} -ml-2.5`}>
          <IconBack aria-hidden className="size-4" />
          <span>{t("instances.detail.backToList")}</span>
        </Link>
      </div>
    );

  const isRunning = running?.some((s) => s.instanceId === instance.id) ?? false;

  return (
    <div className="flex max-w-2xl flex-col gap-6 p-10">
      <header className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h1 className={`${title} truncate`}>{instance.name}</h1>
          <span className="tabular text-sm text-text-muted">{instance.mcVersion}</span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {/* アイコンだけのボタンなので、名前をツールチップでも見せる */}
          {folderButtons.map(({ kind, icon: Icon }) => (
            <TooltipTrigger key={kind} delay={500}>
              <Button
                aria-label={t(`instances.detail.folders.${kind}`)}
                onPress={() => {
                  openInstanceFolder(instance.id, kind).catch(toast.showError);
                }}
                className={iconButton}
              >
                <Icon aria-hidden className="size-4" />
              </Button>
              <Tooltip offset={6} className={tooltipClass}>
                {t(`instances.detail.folders.${kind}`)}
              </Tooltip>
            </TooltipTrigger>
          ))}
          <Link to="/instances" className={quietLink}>
            <IconBack aria-hidden className="size-4" />
            <span>{t("nav.instances")}</span>
          </Link>
        </div>
      </header>
      {/* タブは中身のあるものだけを出す。いまは設定だけである */}
      <Tabs>
        <TabList aria-label={instance.name} className="flex gap-6 border-b border-line">
          <Tab id="settings" className={tabClass}>
            {t("instances.detail.tabSettings")}
          </Tab>
        </TabList>
        <TabPanel id="settings" className="pt-6 outline-none">
          {/* インスタンスが替わったら入力を作り直す */}
          <SettingsTab key={instance.id} instance={instance} running={isRunning} />
        </TabPanel>
      </Tabs>
    </div>
  );
}

// 1段の行は 44px、ラベルと説明の2段の行は 52px（アプリの設定の画面と同じ）
function Row({ children }: { children: ReactNode }) {
  return <div className="flex min-h-11 flex-col justify-center gap-2 py-2">{children}</div>;
}

function Notice({ tone, children }: { tone: "info" | "warning"; children: ReactNode }) {
  const Icon = tone === "warning" ? IconWarning : IconInfo;
  return (
    <p className="flex items-start gap-2 text-sm text-text">
      <Icon aria-hidden className={`mt-0.5 size-4 shrink-0 ${tone === "warning" ? "text-warning" : "text-text-muted"}`} />
      <span>{children}</span>
    </p>
  );
}

function SettingsTab({ instance, running }: { instance: Instance; running: boolean }) {
  const { t } = useTranslation();
  const update = useUpdateInstance();
  // 保存済みの値。保存に成功したら、一覧の取り直しを待たずに差し替える
  const [base, setBase] = useState<SettingsForm>(() => toForm(instance));
  const [form, setForm] = useState<SettingsForm>(base);
  // 保存の結果はトーストで出す
  const toast = useToast();

  // 物理メモリの取得中や失敗のときは、数値の入力だけを出す
  const { data: totalMb } = useQuery({ queryKey: ["system-memory-mb"], queryFn: systemMemoryMb, staleTime: Infinity });
  const maxMb = totalMb === undefined ? null : sliderMax(totalMb);

  const dirty = isDirty(base, form);
  const canSave = dirty && isValid(form) && !update.isPending;
  const modes = [
    { id: "auto", label: t("instances.detail.modeAuto") },
    { id: "manual", label: t("instances.detail.modeManual") },
  ] as const;

  const change = (patch: Partial<SettingsForm>) => {
    setForm((f) => ({ ...f, ...patch }));
  };

  const save = () => {
    if (!canSave) return;
    update.mutate(
      { instanceId: instance.id, settings: toSettings(form) },
      {
        onSuccess: (updated) => {
          const next = toForm(updated);
          setBase(next);
          setForm(next);
          toast.show({ kind: "success", message: t("instances.detail.saved") });
        },
        onError: toast.showError,
      },
    );
  };

  return (
    <form
      className="flex flex-col gap-6"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      {running && <Notice tone="info">{t("instances.detail.nextLaunch")}</Notice>}
      <div className="flex flex-col divide-y divide-line border-y border-line">
        <Row>
          <TextField
            value={form.name}
            onChange={(name) => change({ name })}
            maxLength={64}
            className="flex items-center justify-between gap-4"
          >
            <Label className="text-text">{t("instances.detail.name")}</Label>
            <Input className={`${inputBase} ${controlWidth}`} />
          </TextField>
        </Row>
        <Row>
          <Select<MemoryMode>
            label={t("instances.detail.memory")}
            value={form.memoryMode}
            options={[...modes]}
            onChange={(memoryMode) => change({ memoryMode })}
            triggerClassName={controlWidth}
          />
          {form.memoryMode === "manual" && maxMb !== null && (
            <MemorySlider
              label={t("instances.detail.memorySlider")}
              value={form.memoryMb}
              min={MEMORY_MIN_MB}
              max={maxMb}
              step={SLIDER_STEP_MB}
              onChange={(memoryMb) => change({ memoryMb })}
            />
          )}
          {form.memoryMode === "manual" && (
            <NumberField
              label={t("instances.detail.memoryMb")}
              description={t("instances.detail.memoryRange")}
              value={form.memoryMb}
              min={MEMORY_MIN_MB}
              max={MEMORY_MAX_MB}
              inputWidth="w-16"
              onChange={(memoryMb) => change({ memoryMb })}
            />
          )}
          {memoryError(form) && <Notice tone="warning">{t("instances.detail.memoryRange")}</Notice>}
        </Row>
        <Row>
          <Select<JavaMode>
            label={t("instances.detail.java")}
            value={form.javaMode}
            options={[...modes]}
            onChange={(javaMode) => change({ javaMode })}
            triggerClassName={controlWidth}
          />
          {form.javaMode === "manual" && (
            <TextField value={form.javaPath} onChange={(javaPath) => change({ javaPath })} className="flex flex-col gap-1">
              <Label className="text-text">{t("instances.detail.javaPath")}</Label>
              <Input className={`${inputBase} w-full`} spellCheck={false} />
              <Text slot="description" className="text-xs text-text-muted">
                {t("instances.detail.javaPathHint")}
              </Text>
            </TextField>
          )}
        </Row>
        <Row>
          <TextField value={form.jvmArgs} onChange={(jvmArgs) => change({ jvmArgs })} className="flex flex-col gap-1">
            <Label className="text-text">{t("instances.detail.jvmArgs")}</Label>
            <Input className={`${inputBase} tabular w-full`} spellCheck={false} />
            <Text slot="description" className="text-xs text-text-muted">
              {t("instances.detail.jvmArgsHelp")}
            </Text>
          </TextField>
          {hasMemoryFlag(form.jvmArgs) && <Notice tone="warning">{t("instances.detail.xmxWarning")}</Notice>}
        </Row>
      </div>
      <div className="flex items-center gap-3">
        <Button type="submit" isDisabled={!canSave} isPending={update.isPending} className={saveButton}>
          {t("instances.detail.save")}
        </Button>
      </div>
    </form>
  );
}
