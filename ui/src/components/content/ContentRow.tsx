import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import {
  Button,
  Dialog,
  DialogTrigger,
  Focusable,
  GridListItem,
  ListBox,
  ListBoxItem,
  Popover,
  Tooltip,
  TooltipTrigger,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconResourcePack from "~icons/ph/image";
import IconShader from "~icons/ph/package";
import IconMod from "~icons/ph/puzzle-piece";
import IconUpdate from "~icons/ph/arrow-up";
import IconCaret from "~icons/ph/caret-down";
import IconSpinner from "~icons/ph/circle-notch";
import IconTrash from "~icons/ph/trash";
import IconWarning from "~icons/ph/warning";
import { getContentIcon } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { AvailableUpdate } from "../../bindings/AvailableUpdate";
import type { ContentItem } from "../../bindings/ContentItem";
import type { ContentKind } from "../../bindings/ContentKind";
import type { VersionChoice } from "../../bindings/VersionChoice";
import { useContentVersions } from "../../features/modrinth/useModrinth";
import { FailureNotice } from "../FailureNotice";
import { tooltipClass } from "../tooltip";

/** 1 行の高さ（px）。ListLayout の rowSize と行の h-14 を揃える。 */
export const ROW_SIZE = 56;

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const defaultIcons: Record<ContentKind, typeof IconMod> = {
  mod: IconMod,
  resourcepack: IconResourcePack,
  shader: IconShader,
};

// 一覧の行の操作。面は乗せたときだけ現れる（インスタンスの詳細のアイコンのボタンと同じ）
const iconButton = [
  "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 disabled:opacity-50 disabled:hover:bg-transparent",
  ring,
].join(" ");

// 更新のボタン。行の中で目立ちすぎないよう小さく、文字は短くする（行き先の版はツールチップで見せる）
const updateButton = [
  "inline-flex h-7 shrink-0 items-center gap-1 rounded-md border border-line-control px-1.5 text-xs text-accent @lg:px-2",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3 disabled:opacity-50 disabled:hover:bg-transparent",
  ring,
].join(" ");

// 有効無効のトグル。押されている（有効）ときは溝を明るくし、つまみを右へ寄せる
const toggleClass = [
  "group relative inline-flex h-5 w-8 shrink-0 items-center rounded-full border border-line-control bg-surface-1",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "aria-pressed:border-transparent aria-pressed:bg-accent disabled:opacity-50",
  ring,
].join(" ");

const knobClass = [
  "ml-0.5 size-3.5 rounded-full bg-text-muted",
  "transition-[translate,background-color] duration-(--duration-fast) ease-out-quint",
  "group-aria-pressed:translate-x-3 group-aria-pressed:bg-accent-ink motion-reduce:transition-colors",
].join(" ");

// 版の番号のボタン。文字のように見せ、乗せたときだけ面を出す
const versionButton = [
  "-mx-1 inline-flex h-5 shrink-0 items-center gap-0.5 self-center rounded-sm px-1 text-xs text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 disabled:opacity-50 disabled:hover:bg-transparent",
  ring,
].join(" ");

// 版の一覧の開閉。ボタンの側から 4px 動きながら現れる（インスタンスの一覧のメニューと同じ動き）
const versionPopover = [
  "w-80 rounded-lg border border-line bg-surface-2 p-1 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const versionOption = [
  "flex h-9 cursor-default items-center gap-2 rounded-sm px-2 text-sm text-text outline-none",
  "hover:bg-surface-3 focus:bg-surface-3 disabled:text-text-muted disabled:hover:bg-transparent",
].join(" ");

// 版の種類の札。リリース以外は目に留まるよう色を付ける
const typeBadge: Record<string, string> = {
  release: "border-line text-text-muted",
  beta: "border-warning/50 text-warning",
  alpha: "border-danger/50 text-danger",
};

/**
 * 行の id。有効無効の切り替えでファイル名に付け外しする `.disabled` を除いた relPath にして、
 * 切り替えの前後で同じ行として扱わせる（行を作り直すと、アイコンやフォーカスが一瞬リセットされる）。
 */
export function rowId(relPath: string): string {
  return relPath.endsWith(".disabled") ? relPath.slice(0, -".disabled".length) : relPath;
}

/** 版を切り替えるときに要るもの。Modrinth で特定された MOD の行にだけ使う。 */
export type VersionPickerProps = {
  instanceId: string;
  onChange: (item: ContentItem, choice: VersionChoice) => void;
};

/**
 * 版の番号のボタンと、このインスタンスに合う版の一覧（新しい順）。一覧は開いたときに読む。
 * いまの版は選べない。選んだらすぐに閉じて切り替えを始める（確認は取らない）。
 */
function VersionPicker(
  props: VersionPickerProps & { item: ContentItem; name: string; version: string; busy: boolean },
) {
  const { t, i18n } = useTranslation();
  const { item, name } = props;
  const [open, setOpen] = useState(false);
  const versions = useContentVersions(props.instanceId, item.relPath, open);
  const date = useMemo(() => new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }), [i18n.language]);
  const data = versions.data;
  const current = useMemo(() => (data ?? []).filter((v) => v.current).map((v) => v.versionId), [data]);
  const items = useMemo(() => (data ?? []).map((v) => ({ id: v.versionId, v })), [data]);

  let body;
  if (versions.error && !data)
    body = (
      <div role="alert" className="px-2 py-1.5 text-sm">
        <FailureNotice error={toErrorPayload(versions.error)} />
      </div>
    );
  else if (!data) body = <p className="px-2 py-1.5 text-sm text-text-muted">{t("modrinth.versionsLoading")}</p>;
  else if (data.length === 0) body = <p className="px-2 py-1.5 text-sm text-text-muted">{t("modrinth.noCompatible")}</p>;
  else
    body = (
      <ListBox
        aria-label={t("modrinth.versions", { name })}
        items={items}
        disabledKeys={current}
        autoFocus="first"
        onAction={(key) => {
          const choice = data.find((v) => v.versionId === key);
          if (!choice || choice.current) return;
          setOpen(false);
          props.onChange(item, choice);
        }}
        className="scroll-thin max-h-72 overflow-auto outline-none"
      >
        {({ v }) => (
          <ListBoxItem id={v.versionId} textValue={v.versionNumber} className={versionOption}>
            <span className="tabular min-w-0 flex-1 truncate">{v.versionNumber}</span>
            {v.current && <span className="shrink-0 text-xs text-accent">{t("modrinth.currentVersion")}</span>}
            <span
              className={`shrink-0 rounded-sm border px-1.5 text-xs leading-4 ${typeBadge[v.versionType] ?? typeBadge.release}`}
            >
              {i18n.exists(`modrinth.versionType.${v.versionType}`)
                ? t(`modrinth.versionType.${v.versionType}` as never)
                : v.versionType}
            </span>
            {v.datePublished !== "" && (
              <time dateTime={v.datePublished} className="tabular w-24 shrink-0 text-right text-xs text-text-muted">
                {date.format(new Date(v.datePublished))}
              </time>
            )}
          </ListBoxItem>
        )}
      </ListBox>
    );

  return (
    <DialogTrigger isOpen={open} onOpenChange={setOpen}>
      <Button
        aria-label={t("modrinth.changeVersion", { name, version: props.version })}
        isDisabled={props.busy}
        className={versionButton}
      >
        <span className="tabular">{props.version}</span>
        <IconCaret aria-hidden className="size-3 shrink-0" />
      </Button>
      <Popover placement="bottom start" offset={4} className={versionPopover}>
        <Dialog aria-label={t("modrinth.versions", { name })} className="outline-none">
          {body}
        </Dialog>
      </Popover>
    </DialogTrigger>
  );
}

/** 行のアイコン。Modrinth の URL、jar の中のアイコン、既定の絵の順に使う。 */
function RowIcon({ item }: { item: ContentItem }) {
  // URL が読めなかった（オフラインなど）ときは jar の中のアイコンへ、それも読めなければ既定の絵に戻す
  const [remoteFailed, setRemoteFailed] = useState(false);
  const [localFailed, setLocalFailed] = useState(false);
  const local = useQuery({
    queryKey: ["content-icon", item.sha1],
    queryFn: () => getContentIcon(item.sha1),
    staleTime: Infinity,
    enabled: (item.iconUrl === null || remoteFailed) && item.hasLocalIcon,
  });
  const remote = remoteFailed ? null : item.iconUrl;
  const src = remote ?? (localFailed ? null : (local.data ?? null));
  const Icon = defaultIcons[item.kind];
  // 枠は画像の有無にかかわらず同じ正方形にする。画像は枠に重ねて枠いっぱいに広げ、はみ出しは切り抜く。
  // grid の中の高さ 100% は効かない（画像の縦横比の高さになる）ので、絶対配置で枠に合わせる
  return (
    <span
      data-testid="content-icon"
      className="relative grid aspect-square size-9 shrink-0 place-items-center overflow-hidden rounded-md bg-surface-2 text-text-muted"
    >
      {src !== null ? (
        <img
          key={src}
          src={src}
          alt=""
          width={36}
          height={36}
          loading="lazy"
          onError={() => (remote !== null ? setRemoteFailed(true) : setLocalFailed(true))}
          className="absolute inset-0 block size-full max-w-none object-cover"
        />
      ) : (
        <Icon aria-hidden className="size-4" />
      )}
    </span>
  );
}

export function ContentRow(props: {
  /** 行の id。省くと `rowId(item.relPath)` */
  id?: string;
  item: ContentItem;
  onToggle: (item: ContentItem) => void;
  onRemove: (item: ContentItem) => void;
  busy: boolean;
  /** この行の切り替えの最中。トグルだけを止める */
  toggling?: boolean;
  /** 更新の確認で見つかった更新。無ければ更新のボタンを出さない */
  update?: AvailableUpdate | null;
  /** このインスタンスに合う版が Modrinth に無かったとき、名前の横に控えめな印を出す */
  noCompatible?: boolean;
  /** 更新している最中。ボタンの代わりに「更新中…」を出す */
  updating?: boolean;
  onUpdate?: (item: ContentItem) => void;
  /** 版の切り替え。渡したときだけ、Modrinth で特定された MOD の版の番号をボタンにする */
  versionPicker?: VersionPickerProps;
}) {
  const { t } = useTranslation();
  const { item } = props;
  const name = item.name ?? item.fileName;
  // 版を切り替えられるのは、Modrinth で特定された MOD の行だけ
  const picker =
    item.kind === "mod" && item.provider === "modrinth" && item.projectId !== null && item.versionId !== null
      ? props.versionPicker
      : undefined;

  return (
    <GridListItem
      id={props.id ?? rowId(item.relPath)}
      textValue={name}
      className={`flex h-14 items-center gap-3 rounded-md px-3 outline-none hover:bg-surface-1 ${ring}`}
    >
      {/* 無効の行は中身だけを薄くする。操作のボタンは薄くしない */}
      <div className={`flex min-w-0 flex-1 items-center gap-3 ${item.enabled ? "" : "opacity-50"}`}>
        <RowIcon item={item} />
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex min-w-0 items-baseline gap-2">
            <span className="truncate text-text">{name}</span>
            {item.versionNumber !== null &&
              (picker ? (
                <VersionPicker
                  {...picker}
                  item={item}
                  name={name}
                  version={item.versionNumber}
                  busy={props.busy || (props.updating ?? false)}
                />
              ) : (
                <span className="tabular shrink-0 text-xs text-text-muted">{item.versionNumber}</span>
              ))}
            {props.noCompatible && (
              <TooltipTrigger delay={300}>
                <Focusable>
                  <span
                    role="img"
                    tabIndex={0}
                    aria-label={t("modrinth.noCompatible")}
                    className={`shrink-0 self-center rounded-sm text-warning outline-none ${ring}`}
                  >
                    <IconWarning aria-hidden className="size-3.5" />
                  </span>
                </Focusable>
                <Tooltip offset={6} className={tooltipClass}>
                  {t("modrinth.noCompatible")}
                </Tooltip>
              </TooltipTrigger>
            )}
            {item.installedAs === "dependency" && (
              <span className="shrink-0 self-center rounded-sm border border-line px-1.5 text-xs leading-4 text-text-muted">
                {t("content.row.dependency")}
              </span>
            )}
          </div>
          {item.description !== null && <span className="truncate text-xs text-text-muted">{item.description}</span>}
        </div>
      </div>
      {/* 更新があるときだけ出す。無効の MOD にも出す（更新しても無効のまま） */}
      {props.updating ? (
        <span className="inline-flex h-7 shrink-0 items-center gap-1 px-2 text-xs text-text-muted">
          <IconSpinner aria-hidden className="size-3.5 motion-safe:animate-spin" />
          <span>{t("modrinth.updating")}</span>
        </span>
      ) : (
        props.update && (
          <TooltipTrigger delay={300}>
            <Button
              aria-label={t("modrinth.updateNamed", { name, version: props.update.versionNumber })}
              isDisabled={props.busy}
              onPress={() => props.onUpdate?.(item)}
              className={updateButton}
            >
              <IconUpdate aria-hidden className="size-3.5 shrink-0" />
              {/* 狭い一覧では絵だけにして、名前の幅を残す（版はツールチップで分かる） */}
              <span className="hidden @lg:inline">{t("modrinth.update")}</span>
            </Button>
            <Tooltip offset={6} className={tooltipClass}>
              {t("modrinth.updateTo", { version: props.update.versionNumber })}
            </Tooltip>
          </TooltipTrigger>
        )
      )}
      <Button
        aria-label={t("content.row.enable", { name })}
        aria-pressed={item.enabled}
        isDisabled={props.busy || (props.toggling ?? false)}
        onPress={() => props.onToggle(item)}
        className={toggleClass}
      >
        <span aria-hidden className={knobClass} />
      </Button>
      {/* アイコンだけのボタンなので、名前をツールチップでも見せる */}
      <TooltipTrigger delay={500}>
        <Button
          aria-label={t("content.row.remove", { name })}
          isDisabled={props.busy}
          onPress={() => props.onRemove(item)}
          className={iconButton}
        >
          <IconTrash aria-hidden className="size-4" />
        </Button>
        <Tooltip offset={6} className={tooltipClass}>
          {t("content.row.removeShort")}
        </Tooltip>
      </TooltipTrigger>
    </GridListItem>
  );
}
