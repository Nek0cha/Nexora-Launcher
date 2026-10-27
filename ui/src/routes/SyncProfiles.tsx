import { useState } from "react";
import { Button, Menu, MenuItem, MenuTrigger, Popover, Section, Header } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import IconSliders from "~icons/ph/sliders-horizontal";
import { toErrorPayload } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { SyncMember } from "../bindings/SyncMember";
import { CreateInstanceDialog } from "../components/CreateInstanceDialog";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";
import { InstanceChip } from "../components/sync/InstanceChip";
import { JoinSyncDialog } from "../components/sync/JoinSyncDialog";
import { ProfileCard } from "../components/sync/ProfileCard";
import { useToast } from "../components/toast/useToast";
import { useInstances } from "../features/instances/useInstances";
import { useRunning } from "../features/play/useGameSync";
import { type DropAction, dropAction, useChipDrag } from "../features/sync/chipDrag";
import { useInstanceSync } from "../features/sync/useInstanceSync";
import { useSyncProfileActions, useSyncProfiles } from "../features/sync/useSyncProfiles";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const primaryButton = [
  "inline-flex h-10 items-center gap-2 rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint data-hovered:opacity-90",
  ring,
].join(" ");

const quietButton = [
  "inline-flex h-9 items-center gap-1.5 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint hover:bg-surface-2 pressed:bg-surface-3",
  ring,
].join(" ");

const popoverClass = [
  "max-h-72 min-w-48 overflow-auto rounded-lg border border-line bg-surface-2 p-1 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const menuItemClass =
  "flex h-9 cursor-default items-center truncate rounded-sm px-2 text-sm text-text outline-none hover:bg-surface-3 focused:bg-surface-3";

/** 値の違いを聞いている参加。ダイアログを閉じる動きの間も中身を残すため、開閉と分けて持つ */
type Asking = { instance: SyncMember; profileId: string; differing: number };

/** サイドバーの「共通設定」（spec 3.1 の入口 2）。共通設定ごとのカードと、未参加のインスタンスの一覧 */
export default function SyncProfiles() {
  const { t } = useTranslation();
  const instances = useInstances();
  const profiles = useSyncProfiles();
  const running = useRunning();
  const actions = useSyncProfileActions();
  const toast = useToast();
  const [asking, setAsking] = useState<Asking | null>(null);
  const [askOpen, setAskOpen] = useState(false);
  // 新しく作るときの元のインスタンス
  const [source, setSource] = useState<SyncMember | null>(null);
  const [sourceOpen, setSourceOpen] = useState(false);

  const list = instances.data;
  const all = profiles.data;
  const isRunning = (id: string) => running?.some((s) => s.instanceId === id) ?? false;
  const nameOf = (id: string) => list?.find((i) => i.id === id)?.name ?? "";
  const profileName = (id: string) => all?.find((p) => p.id === id)?.name ?? "";

  const joined = (instanceId: string, profileId: string) => {
    const key = isRunning(instanceId) ? "sync.screen.joinedRunning" : "sync.screen.joined";
    toast.show({ kind: "success", message: t(key, { instance: nameOf(instanceId), profile: profileName(profileId) }) });
  };

  const join = (instanceId: string, profileId: string) => {
    actions.join.mutate(
      { instanceId, profileId },
      {
        onSuccess: (differing) => {
          if (differing === 0) return joined(instanceId, profileId);
          setAsking({ instance: { id: instanceId, name: nameOf(instanceId) }, profileId, differing });
          setAskOpen(true);
        },
      },
    );
  };

  const onDrop = (action: DropAction) => {
    if (action.kind === "join") join(action.instanceId, action.profileId);
    else if (action.kind === "leave") actions.leave.mutate({ instanceId: action.instanceId, name: nameOf(action.instanceId) });
  };
  const { drag, start } = useChipDrag(onDrop);

  const createFrom = (instance: SyncMember) => {
    setSource(instance);
    setSourceOpen(true);
  };

  const title = (
    <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("nav.sync")}</h1>
  );

  const error = (!list && instances.error) || (!all && profiles.error);
  if (error)
    return (
      <div className="flex flex-col gap-8 p-10">
        {title}
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  // 届く前に空の状態を出さない
  if (!list || !all) return null;

  const createDialog = source && (
    <CreateFromInstance
      instance={source}
      running={isRunning(source.id)}
      isOpen={sourceOpen}
      onOpenChange={setSourceOpen}
    />
  );

  if (list.length === 0)
    return (
      <EmptyState icon={IconSliders} title={t("nav.sync")} body={t("sync.screen.noInstancesBody")}>
        <CreateInstanceDialog trigger="primary" />
      </EmptyState>
    );

  if (all.length === 0)
    return (
      <>
        <EmptyState icon={IconSliders} title={t("nav.sync")} body={t("sync.screen.emptyBody")}>
          <CreateMenu instances={list} onPick={createFrom} variant="primary" />
        </EmptyState>
        {createDialog}
      </>
    );

  const members = new Set(all.flatMap((p) => p.members.map((m) => m.id)));
  const unjoined: SyncMember[] = list.filter((i) => !members.has(i.id)).map(({ id, name }) => ({ id, name }));
  // 参加中のチップを未参加の一覧の上へ運んでいるとき、離せば外れることを示す
  const leaveActive = drag !== null && drag.profileId !== null && drag.zone?.kind === "unjoined";
  const dropKind = drag ? dropAction(drag, drag.zone).kind : "none";
  const dropHint =
    dropKind === "join" ? t("sync.screen.dropToJoin") : dropKind === "leave" ? t("sync.screen.dropToLeave") : null;

  return (
    <div className="flex max-w-5xl flex-col gap-6 p-10">
      <header className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-4">
          {title}
          <CreateMenu instances={list} onPick={createFrom} variant="quiet" />
        </div>
        <p className="max-w-2xl text-sm text-text-muted">{t("sync.screen.intro")}</p>
      </header>

      <div className="grid grid-cols-[minmax(0,1fr)_13rem] items-start gap-6">
        <div className="flex flex-col gap-4">
          {all.map((p) => (
            <ProfileCard
              key={p.id}
              profile={p}
              unjoined={unjoined}
              onJoin={join}
              drag={drag}
              onChipDragStart={start}
            />
          ))}
        </div>

        <aside
          aria-label={t("sync.screen.unjoined")}
          data-drop-zone="unjoined"
          className={[
            "sticky top-0 flex flex-col gap-3 rounded-lg border border-dashed p-4",
            "transition-colors duration-(--duration-fast) ease-out-quint",
            leaveActive ? "border-accent bg-surface-2" : "border-line-control",
          ].join(" ")}
        >
          <h2 className="text-xs font-medium tracking-[0.08em] text-text-muted">{t("sync.screen.unjoined")}</h2>
          {unjoined.length > 0 ? (
            <ul className="flex flex-wrap gap-1.5">
              {unjoined.map((i) => (
                <li key={i.id} className="max-w-full">
                  <InstanceChip
                    name={i.name}
                    onDragStart={(e) => start({ instanceId: i.id, profileId: null, name: i.name }, e)}
                    dimmed={drag?.instanceId === i.id}
                  />
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-sm text-text-muted">{t("sync.screen.unjoinedNone")}</p>
          )}
        </aside>
      </div>

      {/* つかんでいるチップと、離したときに起きること。ポインターの下の要素を拾えるよう、自分はポインターを受けない。
          説明をカードの中に出すと、カードの高さが変わって落とし先が動くため、ここに出す */}
      {drag && (
        <div
          data-testid="drag-ghost"
          aria-hidden
          className="pointer-events-none fixed z-50 flex -translate-x-1/2 -translate-y-3.5 flex-col items-center gap-1"
          style={{ left: drag.x, top: drag.y }}
        >
          <span className="inline-flex h-7 items-center rounded-full border border-line-strong bg-surface-2 px-2.5 text-sm text-text-strong shadow-float">
            {drag.name}
          </span>
          {dropHint && (
            <span className="rounded-md bg-surface-3 px-2 py-0.5 text-xs whitespace-nowrap text-text shadow-float">
              {dropHint}
            </span>
          )}
        </div>
      )}

      {asking && (
        <JoinSyncDialog
          instanceId={asking.instance.id}
          instanceName={asking.instance.name}
          // 参加先が決まっているので、作る側の選択肢は出ない
          hasOptions={false}
          running={isRunning(asking.instance.id)}
          target={{ profileId: asking.profileId, differing: asking.differing }}
          onJoined={(profileId) => joined(asking.instance.id, profileId)}
          isOpen={askOpen}
          onOpenChange={setAskOpen}
        />
      )}
      {createDialog}
    </div>
  );
}

/** 「共通設定を作る」。元にするインスタンスをメニューから選ぶ */
function CreateMenu({ instances, onPick, variant }: {
  instances: Instance[];
  onPick: (instance: SyncMember) => void;
  variant: "primary" | "quiet";
}) {
  const { t } = useTranslation();
  return (
    <MenuTrigger>
      <Button className={variant === "primary" ? primaryButton : quietButton}>
        <IconPlus aria-hidden className={variant === "primary" ? "size-5" : "size-4"} />
        <span>{t("sync.screen.create")}</span>
      </Button>
      <Popover placement="bottom start" offset={4} className={popoverClass}>
        <Menu
          aria-label={t("sync.screen.create")}
          className="outline-none"
          onAction={(key) => {
            const picked = instances.find((i) => i.id === key);
            if (picked) onPick({ id: picked.id, name: picked.name });
          }}
        >
          <Section>
            <Header className="px-2 pt-1 pb-1.5 text-xs text-text-muted">{t("sync.screen.createFrom")}</Header>
            {instances.map((i) => (
              <MenuItem key={i.id} id={i.id} className={menuItemClass}>
                {i.name}
              </MenuItem>
            ))}
          </Section>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

/** 選んだインスタンスの状態を読んでから、作成と参加のダイアログを開く */
function CreateFromInstance({ instance, running, isOpen, onOpenChange }: {
  instance: SyncMember;
  running: boolean;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { data } = useInstanceSync(instance.id);
  if (!data) return null;
  return (
    <JoinSyncDialog
      key={instance.id}
      instanceId={instance.id}
      instanceName={instance.name}
      hasOptions={data.hasOptions}
      applicability={data.applicability}
      running={running}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
    />
  );
}
