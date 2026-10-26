import { type PointerEvent, useEffect, useId, useRef, useState } from "react";
import { Button, Input, Menu, MenuItem, MenuTrigger, Popover, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import IconTrash from "~icons/ph/trash";
import type { Category } from "../../bindings/Category";
import type { SyncMember } from "../../bindings/SyncMember";
import type { SyncProfile } from "../../bindings/SyncProfile";
import type { DragSource, DragState } from "../../features/sync/chipDrag";
import { CATEGORIES, useSyncProfileActions } from "../../features/sync/useSyncProfiles";
import { ConfirmDialog } from "../ConfirmDialog";
import { ChipGroup } from "../content/ContentFilter";
import { InstanceChip } from "./InstanceChip";
import { ProposalsSection } from "./ProposalList";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const secondaryButton = [
  "inline-flex h-8 items-center gap-1.5 rounded-md border border-line-control px-2.5 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50 data-disabled:hover:bg-transparent",
  ring,
].join(" ");

// 控えめな操作。面は乗せたときだけ現れる（一覧の行の操作と同じ）
const quietButton = [
  "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

// 名前は見出しの見た目のまま、その場で書き換えられる。枠は乗せたときとフォーカスしたときだけ出す
const nameInput = [
  "-ml-2 h-9 w-full min-w-0 rounded-md border border-transparent bg-transparent px-2",
  "font-display text-lg font-semibold text-text-strong",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:border-line-control data-focused:border-line-control data-focused:bg-surface-1",
  "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
].join(" ");

const popoverClass = [
  "max-h-72 min-w-44 overflow-auto rounded-lg border border-line bg-surface-2 p-1 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const menuItemClass =
  "flex h-9 cursor-default items-center truncate rounded-sm px-2 text-sm text-text outline-none hover:bg-surface-3 focused:bg-surface-3";

/**
 * 共通設定 1 つ分のカード（spec 3.1 の入口 2）。名前、揃えるカテゴリ、参加中のインスタンス、追加と削除。
 * カード全体がチップの落とし先になる（data-drop-zone）。
 */
export function ProfileCard({ profile, unjoined, onJoin, drag = null, onChipDragStart }: {
  profile: SyncProfile;
  /** どの共通設定にも参加していないインスタンス。「インスタンスを追加」のメニューに出す */
  unjoined: SyncMember[];
  onJoin: (instanceId: string, profileId: string) => void;
  drag?: DragState | null;
  onChipDragStart?: (source: DragSource & { name: string }, event: PointerEvent) => void;
}) {
  const { t } = useTranslation();
  const membersId = useId();
  const addNoteId = useId();
  const actions = useSyncProfileActions();
  const [deleteOpen, setDeleteOpen] = useState(false);

  const pendingCategories =
    actions.categories.isPending && actions.categories.variables ? actions.categories.variables.next : null;
  const selected = pendingCategories ?? profile.categories;

  // 別のところから来たチップがこのカードの上にあるとき、離せば参加することを示す
  const dropActive =
    drag !== null &&
    drag.zone?.kind === "profile" &&
    drag.zone.profileId === profile.id &&
    drag.profileId !== profile.id;

  return (
    <section
      aria-label={profile.name}
      data-drop-zone="profile"
      data-profile-id={profile.id}
      data-drop-active={dropActive ? "true" : undefined}
      className={[
        "flex flex-col gap-4 rounded-lg border bg-surface-1 p-4",
        "transition-colors duration-(--duration-fast) ease-out-quint",
        dropActive ? "border-accent bg-surface-2" : "border-line",
      ].join(" ")}
    >
      <div className="flex items-center gap-2">
        <ProfileName
          profile={profile}
          onRename={(name, onFailed) =>
            actions.rename.mutate({ profileId: profile.id, name }, { onError: onFailed })
          }
        />
        <Button
          aria-label={t("sync.screen.deleteLabel", { name: profile.name })}
          onPress={() => setDeleteOpen(true)}
          className={quietButton}
        >
          <IconTrash aria-hidden className="size-4" />
          <span>{t("sync.screen.delete")}</span>
        </Button>
      </div>

      <ChipGroup<Category>
        label={t("sync.screen.categories")}
        options={CATEGORIES.map((id) => ({ id, label: t(`sync.categories.${id}`) }))}
        selected={selected}
        onChange={(next) => actions.categories.mutate({ profileId: profile.id, next })}
      />

      <div className="flex flex-col gap-2">
        <span id={membersId} className="text-xs text-text-muted">
          {t("sync.screen.members")}
        </span>
        {profile.members.length > 0 ? (
          <ul aria-labelledby={membersId} className="flex flex-wrap gap-1.5">
            {profile.members.map((m) => (
              <li key={m.id} className="max-w-full">
                <InstanceChip
                  name={m.name}
                  onRemove={() => actions.leave.mutate({ instanceId: m.id, name: m.name })}
                  removeDisabled={actions.leave.isPending}
                  onDragStart={
                    onChipDragStart
                      ? (e) => onChipDragStart({ instanceId: m.id, profileId: profile.id, name: m.name }, e)
                      : undefined
                  }
                  dimmed={drag?.instanceId === m.id}
                />
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-text-muted">{t("sync.screen.membersNone")}</p>
        )}
      </div>

      {/* ゲームの中で変えた設定。0 件なら何も出ない */}
      <ProposalsSection profileId={profile.id} members={profile.members} />

      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <MenuTrigger>
          <Button
            isDisabled={unjoined.length === 0}
            aria-describedby={unjoined.length === 0 ? addNoteId : undefined}
            className={secondaryButton}
          >
            <IconPlus aria-hidden className="size-4" />
            <span>{t("sync.screen.add")}</span>
          </Button>
          <Popover placement="bottom start" offset={4} className={popoverClass}>
            <Menu
              aria-label={t("sync.screen.add")}
              className="outline-none"
              onAction={(key) => onJoin(String(key), profile.id)}
            >
              {unjoined.map((i) => (
                <MenuItem key={i.id} id={i.id} className={menuItemClass}>
                  {i.name}
                </MenuItem>
              ))}
            </Menu>
          </Popover>
        </MenuTrigger>
        {unjoined.length === 0 && (
          <p id={addNoteId} className="text-xs text-text-muted">
            {t("sync.screen.addNone")}
          </p>
        )}
      </div>

      <ConfirmDialog
        title={t("sync.screen.deleteTitle", { name: profile.name })}
        confirmLabel={t("sync.screen.deleteConfirm")}
        cancelLabel={t("sync.screen.cancel")}
        tone="danger"
        isOpen={deleteOpen}
        onOpenChange={setDeleteOpen}
        isPending={actions.remove.isPending}
        onConfirm={() =>
          actions.remove.mutate(
            { profileId: profile.id, name: profile.name },
            { onSettled: () => setDeleteOpen(false) },
          )
        }
      >
        <p>{t("sync.screen.deleteBody")}</p>
      </ConfirmDialog>
    </section>
  );
}

/** その場で変える名前。Enter かフォーカスが外れたときに保存し、Esc と空の名前は元に戻す */
function ProfileName({ profile, onRename }: {
  profile: SyncProfile;
  /** 失敗したら onFailed を呼ぶ */
  onRename: (name: string, onFailed: () => void) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<string | null>(null);
  // 送った名前。Enter の後にフォーカスが外れても、同じ名前を二度送らない
  const sent = useRef<string | null>(null);

  // 保存した名前が届いたら（または取り直しで変わったら）、入力中の値を手放す
  useEffect(() => {
    setDraft(null);
    sent.current = null;
  }, [profile.name]);

  const commit = () => {
    if (draft === null) return;
    const name = draft.trim();
    if (name === "" || name === profile.name) {
      setDraft(null);
      return;
    }
    if (name === sent.current) return;
    sent.current = name;
    onRename(name, () => {
      sent.current = null;
      setDraft(null);
    });
  };

  return (
    <TextField
      aria-label={t("sync.screen.name")}
      value={draft ?? profile.name}
      onChange={setDraft}
      maxLength={64}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        else if (e.key === "Escape") setDraft(null);
        else e.continuePropagation();
      }}
      className="min-w-0 flex-1"
    >
      <Input className={nameInput} />
    </TextField>
  );
}
