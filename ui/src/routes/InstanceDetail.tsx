import { useState } from "react";
import {
  Button,
  Input,
  Label,
  Tab,
  TabList,
  TabPanel,
  Tabs,
  TextField,
  Tooltip,
  TooltipTrigger,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router";
import IconBack from "~icons/ph/arrow-left";
import IconGame from "~icons/ph/folder-open";
import IconScreenshots from "~icons/ph/image";
import IconMods from "~icons/ph/puzzle-piece";
import IconWorlds from "~icons/ph/globe-hemisphere-west";
import { openInstanceFolder } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { FolderKind } from "../bindings/FolderKind";
import type { Instance } from "../bindings/Instance";
import { FailureNotice } from "../components/FailureNotice";
import { InstanceAdvancedFields } from "../components/InstanceAdvancedFields";
import { controlWidth, inputBase, Notice, Row } from "../components/settingsLayout";
import { useToast } from "../components/toast/useToast";
import { tooltipClass } from "../components/tooltip";
import {
  isDirty,
  isValid,
  type SettingsForm,
  toForm,
  toSettings,
} from "../features/instances/settingsForm";
import { useUpdateInstance } from "../features/instances/useInstanceActions";
import { useInstances } from "../features/instances/useInstances";
import { useRunning } from "../features/play/useGameSync";
import { ContentTab } from "./ContentTab";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 見出しの横の控えめなリンク。面は乗せたときだけ現れる（一覧の行の操作と同じ）
const quietLink = [
  "inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
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

type DetailTab = "settings" | "content";

export default function InstanceDetail() {
  const { t } = useTranslation();
  const { id } = useParams();
  const { data, error } = useInstances();
  const running = useRunning();
  // フォルダを開けなかったときはトーストで出す
  const toast = useToast();
  const [tab, setTab] = useState<DetailTab>("settings");

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

  // コンテンツのタブでは一覧を広く見せる。高さを画面に合わせ、一覧はその中でスクロールする
  const layout = tab === "content" ? "h-full min-h-[34rem] max-w-5xl" : "max-w-2xl";

  return (
    <div className={`flex ${layout} flex-col gap-6 p-10`}>
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
      {/* タブは中身のあるものだけを出す。スクリーンショットとログは後のフェーズで足す */}
      <Tabs
        selectedKey={tab}
        onSelectionChange={(key) => setTab(key as DetailTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabList aria-label={instance.name} className="flex gap-6 border-b border-line">
          <Tab id="settings" className={tabClass}>
            {t("instances.detail.tabSettings")}
          </Tab>
          <Tab id="content" className={tabClass}>
            {t("instances.detail.tabContent")}
          </Tab>
        </TabList>
        {/* 設定は、タブを切り替えても保存前の入力を残すよう、隠すだけにする */}
        <TabPanel id="settings" shouldForceMount className="pt-6 outline-none data-inert:hidden">
          {/* インスタンスが替わったら入力を作り直す */}
          <SettingsTab key={instance.id} instance={instance} running={isRunning} />
        </TabPanel>
        {/* コンテンツは開くたびに作り直す（開いたときにディスクと照合し、特定する） */}
        <TabPanel id="content" className="flex min-h-0 flex-1 flex-col pt-6 outline-none">
          <ContentTab key={instance.id} instance={instance} running={isRunning} />
        </TabPanel>
      </Tabs>
    </div>
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

  const dirty = isDirty(base, form);
  const canSave = dirty && isValid(form) && !update.isPending;

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
        <InstanceAdvancedFields form={form} onChange={change} />
      </div>
      <div className="flex items-center gap-3">
        <Button type="submit" isDisabled={!canSave} isPending={update.isPending} className={saveButton}>
          {t("instances.detail.save")}
        </Button>
      </div>
    </form>
  );
}
