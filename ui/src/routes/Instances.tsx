import { useId, useMemo, useState } from "react";
import { Button, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import IconMore from "~icons/ph/dots-three-bold";
import IconContent from "~icons/ph/puzzle-piece";
import IconEmpty from "~icons/ph/stack";
import { openInstanceFolder } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import { ConfirmDeleteDialog } from "../components/ConfirmDeleteDialog";
import { CreateInstanceDialog } from "../components/CreateInstanceDialog";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";
import { PlayButton } from "../components/PlayButton";
import { useToast } from "../components/toast/useToast";
import { errorSentence } from "../features/errors/errorText";
import { activeDuplicate, duplicatePercent, useDuplicateRefresh } from "../features/instances/duplicateState";
import { useDeleteInstance, useDuplicateInstance } from "../features/instances/useInstanceActions";
import { useInstances } from "../features/instances/useInstances";
import { useTasks } from "../features/tasks/useTasks";
import i18n from "../i18n";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 行の中の控えめな操作。面は乗せたときだけ現れる（アカウントの画面と同じ）
const rowButton = [
  "inline-flex h-9 items-center justify-center rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  "data-disabled:opacity-50 data-disabled:hover:bg-transparent data-disabled:hover:text-text-muted",
  ring,
].join(" ");

// メニューの開閉。ボタンの側から 4px 動きながら現れる（Select と同じ動き）
const popoverClass = [
  "min-w-36 rounded-lg border border-line bg-surface-2 p-1 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

// 「コンテンツ」のリンク。行の主な行き先なので、枠を付けて控えめな操作より目立たせる
const contentLink = [
  "inline-flex h-9 items-center gap-1.5 rounded-md border border-line-control px-2.5 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  ring,
].join(" ");

// 名前のリンク。見た目は名前のまま、乗せたときだけ下線を出す
const nameLink = [
  "min-w-0 self-start truncate rounded-sm text-text-strong underline-offset-2 hover:underline",
  ring,
].join(" ");

const menuItemClass =
  "flex h-9 cursor-default items-center rounded-sm px-2 text-sm outline-none hover:bg-surface-3 focused:bg-surface-3";

export default function Instances() {
  const { t } = useTranslation();
  const { data, error } = useInstances();
  const tasks = useTasks();
  useDuplicateRefresh();
  const remove = useDeleteInstance();
  const duplicate = useDuplicateInstance();
  // 行の操作の失敗はトーストで出す。一覧の並びを動かさない
  const toast = useToast();
  // 削除の確認。閉じる動きの間も名前を残すため、対象と開閉を分けて持つ
  const [deleting, setDeleting] = useState<Instance | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const title = (
    <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("nav.instances")}</h1>
  );

  // 取り直しの失敗では、手元の一覧を出したままにする
  if (!data && error)
    return (
      <div className="flex flex-col gap-8 p-10">
        {title}
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  // 届く前に空の状態を出すと、インスタンスがあるのに一瞬「まだありません」と見えるため、何も出さない
  if (!data) return null;

  const onDuplicate = (instance: Instance) => {
    duplicate.mutate(instance.id, { onError: toast.showError });
  };

  const onOpenFolder = (instance: Instance) => {
    openInstanceFolder(instance.id, "game").catch(toast.showError);
  };

  const onDelete = (instance: Instance) => {
    setDeleting(instance);
    setDeleteOpen(true);
  };

  const confirmDelete = () => {
    if (!deleting || remove.isPending) return;
    remove.mutate(deleting.id, {
      onError: (e) => {
        const payload = toErrorPayload(e);
        // ディレクトリの削除の途中で失敗すると、一部のファイルだけが消えていることがある。同じトーストに続けて書く
        if (payload.code !== "io.failed") return toast.showError(payload);
        const sentence = errorSentence(payload, (key, options) => t(key as never, options), (key) => i18n.exists(key));
        toast.show({ kind: "error", message: `${sentence} ${t("instances.deletePartial")}` });
      },
      onSettled: () => setDeleteOpen(false),
    });
  };

  const dialog = (
    <ConfirmDeleteDialog
      name={deleting?.name ?? ""}
      isOpen={deleteOpen}
      onOpenChange={setDeleteOpen}
      isPending={remove.isPending}
      onConfirm={confirmDelete}
    />
  );

  if (data.length === 0)
    return (
      <>
        <EmptyState icon={IconEmpty} title={t("nav.instances")} body={t("empty.instances")}>
          <CreateInstanceDialog trigger="primary" />
        </EmptyState>
        {dialog}
      </>
    );

  return (
    <div className="flex max-w-4xl flex-col gap-6 p-10">
      <header className="flex items-center justify-between gap-4">
        {title}
        <CreateInstanceDialog trigger="quiet" />
      </header>
      <ul className="flex flex-col divide-y divide-line border-y border-line">
        {data.map((instance) => {
          const task = activeDuplicate(tasks, instance.id);
          // コマンドの応答を待つ間も、タスクが表に届くまでの間の分として、操作は止める。
          // ただし進捗（複製中 0%）は出さない。コマンドが断られる（起動中など）こともあり、受け付けられてタスクが届いてから出す
          const sending = duplicate.isPending && duplicate.variables === instance.id;
          return (
            <InstanceRow
              key={instance.id}
              instance={instance}
              duplicating={task ? duplicatePercent(task) : null}
              sending={sending}
              onDuplicate={() => onDuplicate(instance)}
              onOpenFolder={() => onOpenFolder(instance)}
              onDelete={() => onDelete(instance)}
            />
          );
        })}
      </ul>
      {dialog}
    </div>
  );
}

function useFormatters() {
  const { i18n } = useTranslation();
  const lang = i18n.language;
  return useMemo(() => {
    const date = new Intl.DateTimeFormat(lang, { dateStyle: "medium", timeStyle: "short" });
    const hours = new Intl.NumberFormat(lang, { style: "unit", unit: "hour", unitDisplay: "short" });
    const minutes = new Intl.NumberFormat(lang, { style: "unit", unit: "minute", unitDisplay: "short" });
    const duration = (ms: number) => {
      const total = Math.floor(Math.max(0, ms) / 60_000);
      const h = Math.floor(total / 60);
      const m = total % 60;
      if (h === 0) return minutes.format(m);
      if (m === 0) return hours.format(h);
      return `${hours.format(h)} ${minutes.format(m)}`;
    };
    return { date: (ms: number) => date.format(ms), duration };
  }, [lang]);
}

function StateLabel({ state }: { state: Instance["state"] }) {
  const { t } = useTranslation();
  if (state === "broken")
    return (
      <span className="flex items-center gap-1.5 text-text">
        <span aria-hidden className="size-2 shrink-0 rounded-full bg-warning" />
        <span>{t("instances.state.broken")}</span>
      </span>
    );
  return <span>{t(`instances.state.${state}`)}</span>;
}

function InstanceRow({
  instance,
  duplicating,
  sending,
  onDuplicate,
  onOpenFolder,
  onDelete,
}: {
  instance: Instance;
  /** 複製の進捗（%）。複製していなければ null */
  duplicating: number | null;
  /** 複製のコマンドの応答待ち。操作は止めるが、進捗は出さない */
  sending: boolean;
  onDuplicate: () => void;
  onOpenFolder: () => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  const nameId = useId();
  const format = useFormatters();
  const progressing = duplicating !== null;
  const busy = progressing || sending;
  const dot = <span aria-hidden className="text-text-faint">·</span>;

  return (
    <li aria-labelledby={nameId} className="relative flex min-h-16 items-center gap-4 py-3">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        {/* 名前からも詳細（コンテンツのタブ）を開ける */}
        <Link id={nameId} to={`/instances/${instance.id}`} className={nameLink}>
          {instance.name}
        </Link>
        <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-sm text-text-muted">
          <span className="tabular">{instance.mcVersion}</span>
          {dot}
          <StateLabel state={instance.state} />
          {dot}
          {instance.lastPlayedAt === null ? (
            <span>{t("instances.neverPlayed")}</span>
          ) : (
            <>
              <span className="flex gap-1.5">
                <span className="text-text-faint">{t("instances.lastPlayed")}</span>
                <time dateTime={new Date(instance.lastPlayedAt).toISOString()}>{format.date(instance.lastPlayedAt)}</time>
              </span>
              {dot}
              <span className="flex gap-1.5">
                <span className="text-text-faint">{t("instances.playTime")}</span>
                <span className="tabular">{format.duration(instance.playTimeMs)}</span>
              </span>
            </>
          )}
        </span>
        {progressing && (
          <span className="text-sm text-text">
            <span className="tabular">{t("instances.duplicatingPercent", { percent: duplicating })}</span>
          </span>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <PlayButton instance={instance} size="row" isDisabled={busy} />
        <Link to={`/instances/${instance.id}`} className={contentLink}>
          <IconContent aria-hidden className="size-4 shrink-0" />
          <span>{t("instances.content")}</span>
        </Link>
        {/* 詳細の設定のタブを開く */}
        <Link to={`/instances/${instance.id}`} state={{ tab: "settings" }} className={rowButton}>
          {t("instances.settings")}
        </Link>
        <MenuTrigger>
          <Button aria-label={t("instances.more")} isDisabled={busy} className={`${rowButton} w-9 px-0`}>
            <IconMore aria-hidden className="size-4" />
          </Button>
          <Popover placement="bottom end" offset={4} className={popoverClass}>
            <Menu
              aria-label={t("instances.more")}
              className="outline-none"
              onAction={(key) => (key === "duplicate" ? onDuplicate() : key === "openFolder" ? onOpenFolder() : onDelete())}
            >
              <MenuItem id="duplicate" className={`${menuItemClass} text-text`}>
                {t("instances.duplicate")}
              </MenuItem>
              <MenuItem id="openFolder" className={`${menuItemClass} text-text`}>
                {t("instances.openFolder")}
              </MenuItem>
              <MenuItem id="delete" className={`${menuItemClass} text-danger`}>
                {t("instances.delete")}
              </MenuItem>
            </Menu>
          </Popover>
        </MenuTrigger>
      </div>
      {/* 複製の進捗は行の下端の細い線でも示す。読み上げは上の文で行う */}
      {progressing && (
        <span aria-hidden className="absolute inset-x-0 bottom-0 h-0.5 overflow-hidden">
          <span
            className="block h-full origin-left bg-accent transition-transform duration-(--duration-slow) ease-out-quint"
            style={{ transform: `scaleX(${duplicating / 100})` }}
          />
        </span>
      )}
    </li>
  );
}
