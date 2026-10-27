import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { Button, Dialog, Heading, Input, Modal, ModalOverlay, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconPlus from "~icons/ph/plus";
import IconEmpty from "~icons/ph/user-circle";
import { removeAccount, setActiveAccount, setDevOfflineActive, setDevOfflineUsername } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import type { Account } from "../bindings/Account";
import type { AccountList } from "../bindings/AccountList";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";
import { modalClass, overlayClass } from "../components/modal";
import { useToast } from "../components/toast/useToast";
import { accountStatus } from "../features/accounts/accountState";
import { ChangeSkinDialog } from "../features/accounts/ChangeSkinDialog";
import { SkinFace } from "../features/accounts/SkinFace";
import { ACCOUNTS_KEY, useAccounts } from "../features/accounts/useAccounts";
import { LoginDialog } from "../features/login/LoginDialog";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 空の状態の主な操作。インスタンスの作成と同じ明暗の反転の塗り
const primaryButton = [
  "play-press inline-flex h-11 items-center gap-2 rounded-lg bg-text-strong px-5",
  "font-display font-semibold text-surface-0 data-hovered:opacity-90",
  ring,
].join(" ");

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3 data-pending:cursor-progress data-disabled:opacity-50",
  ring,
].join(" ");

// 行の中の控えめな操作。面は乗せたときだけ現れる
const rowButton = [
  "inline-flex h-8 items-center rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-pending:cursor-progress",
  ring,
].join(" ");

// 削除の確認の主の操作
const dangerButton = [
  "inline-flex h-9 items-center rounded-md bg-danger px-4 text-sm font-medium text-danger-ink",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

type LoginTarget = { mode: "add" } | { mode: "relogin"; accountId: string };

export default function Accounts() {
  const { t } = useTranslation();
  const { data, error } = useAccounts();
  const client = useQueryClient();
  // 行の操作の失敗はトーストで出す。一覧の並びを動かさない
  const toast = useToast();
  // ログインのダイアログ。閉じる動きの間も中身を残すため、対象と開閉を分けて持つ
  const [login, setLogin] = useState<LoginTarget>({ mode: "add" });
  const [loginOpen, setLoginOpen] = useState(false);
  const [removing, setRemoving] = useState<Account | null>(null);
  const [removeOpen, setRemoveOpen] = useState(false);

  // 一覧は accounts://changed でも取り直されるが、届く前に古い表示が残らないよう、成功したらすぐ取り直す
  const refresh = () => void client.invalidateQueries({ queryKey: ACCOUNTS_KEY });
  const action = {
    onSuccess: refresh,
    onError: (e: unknown) => toast.showError(e),
  };
  const activate = useMutation({ mutationFn: (id: string) => setActiveAccount(id), ...action });
  const activateDev = useMutation({ mutationFn: () => setDevOfflineActive(true), ...action });
  // 削除の失敗（ゲームの実行中の auth.account_in_use など）も、トーストで出す
  const remove = useMutation({ mutationFn: (id: string) => removeAccount(id), ...action });

  const openLogin = (target: LoginTarget) => {
    setLogin(target);
    setLoginOpen(true);
  };

  const title = <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("nav.accounts")}</h1>;

  // 取り直しの失敗では、手元の一覧を出したままにする（開いているダイアログを外さない）
  if (!data && error)
    return (
      <div className="flex max-w-2xl flex-col gap-8 p-10">
        {title}
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      </div>
    );
  if (!data) return null;

  const addButton = (primary: boolean) => (
    <Button onPress={() => openLogin({ mode: "add" })} className={primary ? primaryButton : secondaryButton}>
      <IconPlus aria-hidden className={primary ? "size-5" : "size-4"} />
      <span>{t("accounts.add")}</span>
    </Button>
  );

  const dialogs = (
    <>
      {/* 閉じると LoginDialog の中身が外れ、動いているログインはキャンセルされる */}
      <LoginDialog
        mode={login.mode}
        prompt={login.mode === "relogin" ? { kind: "loginRequired", accountId: login.accountId } : null}
        isOpen={loginOpen}
        onClose={() => setLoginOpen(false)}
      />
      <RemoveDialog
        account={removing}
        isOpen={removeOpen}
        isPending={remove.isPending}
        onCancel={() => setRemoveOpen(false)}
        onConfirm={(id) => remove.mutate(id, { onSettled: () => setRemoveOpen(false) })}
      />
    </>
  );

  const content =
    data.accounts.length === 0 && data.devOffline === null ? (
      <EmptyState icon={IconEmpty} title={t("nav.accounts")} body={t("empty.accounts")}>
        {addButton(true)}
      </EmptyState>
    ) : (
      <div className="flex max-w-2xl flex-col gap-6 p-10">
        {title}
        <ul className="flex flex-col divide-y divide-line border-y border-line">
          {data.accounts.map((account) => (
            <AccountRow
              key={account.id}
              account={account}
              activating={activate.isPending && activate.variables === account.id}
              onUse={() => activate.mutate(account.id)}
              onRelogin={() => openLogin({ mode: "relogin", accountId: account.id })}
              onRemove={() => {
                setRemoving(account);
                setRemoveOpen(true);
              }}
            />
          ))}
          {/* 製品版の backend は devOffline を常に null で返すため、開発ビルドでだけ出る */}
          {data.devOffline && (
            <DevOfflineRow
              username={data.devOffline.username}
              active={data.devOffline.active}
              activating={activateDev.isPending}
              onUse={() => activateDev.mutate()}
            />
          )}
        </ul>
        <div>{addButton(false)}</div>
      </div>
    );

  // ダイアログは中身の外の決まった位置に置く。空の状態と一覧が入れ替わっても作り直されず、動いているログインが続く
  return (
    <>
      {content}
      {dialogs}
    </>
  );
}

/**
 * 「使う」。使用中の行では押せないので、同じ幅の見えない枠に替えて、隣のボタンの位置を変えない。
 * 枠は読み上げとフォーカスから外す。
 */
function UseButton({ active, activating, onUse, className }: {
  active: boolean;
  activating: boolean;
  onUse: () => void;
  className: string;
}) {
  const { t } = useTranslation();
  if (active)
    return (
      <span aria-hidden data-action-slot className={`${rowButton} invisible ${className}`}>
        {t("accounts.use")}
      </span>
    );
  return (
    <Button onPress={onUse} isPending={activating} className={`${rowButton} ${className}`}>
      {t("accounts.use")}
    </Button>
  );
}

function StatusBadges({ active, problem }: { active: boolean; problem: string | null }) {
  const { t } = useTranslation();
  return (
    <span className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-sm">
      {active && <span className="text-text-muted">{t("accounts.status.active")}</span>}
      {problem && (
        <span className="flex items-center gap-1.5 text-text">
          <span aria-hidden className="size-2 shrink-0 rounded-full bg-warning" />
          {problem}
        </span>
      )}
    </span>
  );
}

function AccountRow({
  account,
  activating,
  onUse,
  onRelogin,
  onRemove,
}: {
  account: Account;
  activating: boolean;
  onUse: () => void;
  onRelogin: () => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const nameId = useId();
  const status = accountStatus(account);
  const problem =
    status === "needsLogin" || status === "unsaved" || status === "ownershipUnknown"
      ? t(`accounts.status.${status}`)
      : null;
  return (
    <li aria-labelledby={nameId} className="flex min-h-15 items-center gap-3 py-2">
      <SkinFace accountId={account.id} size={32} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span id={nameId} className="truncate text-text-strong">
          {account.username}
        </span>
        <StatusBadges active={account.isActive} problem={problem} />
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {account.needsLogin && (
          <Button onPress={onRelogin} className={rowButton}>
            {t("accounts.relogin")}
          </Button>
        )}
        <UseButton active={account.isActive} activating={activating} onUse={onUse} className="" />
        {/* スキンの変更は Microsoft アカウントの行だけに置く。ログインし直しが必要な間は押せない */}
        <ChangeSkinDialog accountId={account.id} isDisabled={account.needsLogin} />
        <Button onPress={onRemove} className={rowButton}>
          {t("accounts.remove")}
        </Button>
      </div>
    </li>
  );
}

/** 開発用のオフラインアカウントの行。フェーズ 2 の名前の入力欄をここに置く。 */
function DevOfflineRow({
  username,
  active,
  activating,
  onUse,
}: {
  username: string;
  active: boolean;
  activating: boolean;
  onUse: () => void;
}) {
  const { t } = useTranslation();
  const titleId = useId();
  return (
    <li aria-labelledby={titleId} className="flex flex-col gap-3 py-3">
      <div className="flex items-center gap-3">
        <SkinFace accountId={null} size={32} />
        <div className="flex min-w-0 flex-1 flex-col">
          <span id={titleId} className="truncate text-text-strong">
            {t("accounts.devTitle")}
          </span>
          <StatusBadges active={active} problem={null} />
        </div>
        <UseButton active={active} activating={activating} onUse={onUse} className="shrink-0" />
      </div>
      <p className="text-sm text-text-muted">{t("accounts.devHint")}</p>
      <DevUsernameForm initial={username} labelledBy={titleId} />
    </li>
  );
}

function DevUsernameForm({ initial, labelledBy }: { initial: string; labelledBy: string }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [value, setValue] = useState(initial);
  const [saved, setSaved] = useState(false);
  const save = useMutation({
    mutationFn: (name: string) => setDevOfflineUsername(name),
    onSuccess: (name) => {
      // 名前の保存では accounts://changed が届かないため、一覧の控えを直す
      client.setQueryData<AccountList>(ACCOUNTS_KEY, (list) =>
        list?.devOffline ? { ...list, devOffline: { ...list.devOffline, username: name } } : list,
      );
      setValue(name);
      setSaved(true);
    },
  });

  return (
    <div className="flex flex-col gap-3">
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (save.isPending) return;
          setSaved(false);
          save.mutate(value);
        }}
      >
        <TextField
          aria-labelledby={labelledBy}
          value={value}
          onChange={(next) => {
            setValue(next);
            setSaved(false);
            // 直し始めたら、前の保存の失敗は消す
            if (save.isError) save.reset();
          }}
          maxLength={16}
        >
          <Input
            spellCheck={false}
            className={[
              "h-9 w-48 rounded-md border border-line-control bg-surface-1 px-3 text-text",
              "transition-colors duration-(--duration-fast) ease-out-quint",
              "data-hovered:border-text-muted",
              "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
            ].join(" ")}
          />
        </TextField>
        <Button type="submit" isPending={save.isPending} className={secondaryButton}>
          {t("accounts.save")}
        </Button>
        <span role="status" className="flex items-center gap-1.5 text-sm text-text-muted">
          {saved && (
            <>
              <IconCheck aria-hidden className="size-4 text-accent" />
              {t("accounts.saved")}
            </>
          )}
        </span>
      </form>
      {save.error && (
        <div role="alert">
          <FailureNotice error={toErrorPayload(save.error)} />
        </div>
      )}
    </div>
  );
}

/** 削除の確認（spec 4.10）。発行済みのトークンは無効にならないことを伝える。 */
function RemoveDialog({
  account,
  isOpen,
  isPending,
  onCancel,
  onConfirm,
}: {
  account: Account | null;
  isOpen: boolean;
  isPending: boolean;
  onCancel: () => void;
  onConfirm: (id: string) => void;
}) {
  const { t } = useTranslation();
  const bodyId = useId();
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={(open) => {
        if (!open && !isPending) onCancel();
      }}
      isDismissable={!isPending}
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-4 p-5 outline-hidden">
          <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
            {t("accounts.removeTitle", { name: account?.username ?? "" })}
          </Heading>
          <p id={bodyId} className="text-sm text-text">
            {t("accounts.removeBody")}
          </p>
          <div className="flex justify-end gap-2 pt-1">
            <Button autoFocus onPress={onCancel} isDisabled={isPending} className={secondaryButton}>
              {t("accounts.cancel")}
            </Button>
            <Button
              onPress={() => account && onConfirm(account.id)}
              isPending={isPending}
              className={dangerButton}
            >
              {t("accounts.removeConfirm")}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
