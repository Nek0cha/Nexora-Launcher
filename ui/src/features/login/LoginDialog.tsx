import { useEffect, useRef, useState, useSyncExternalStore, type RefObject, type ReactNode } from "react";
import { Button, Dialog, Heading, Link, Modal, ModalOverlay } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconExternal from "~icons/ph/arrow-square-out";
import IconCopy from "~icons/ph/copy";
import { openExternal } from "../../api/opener";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import { FailureNotice } from "../../components/FailureNotice";
import { modalClass, overlayClass } from "../../components/modal";
import { SkinFace } from "../accounts/SkinFace";
import { pickActive, useAccounts } from "../accounts/useAccounts";
import { usePlay, type Play } from "../play/usePlay";
import {
  isAllowedVerificationUri,
  PROFILE_URL,
  PURCHASE_URL,
  sameAccount,
  TRIAL_URL,
  type LoginPrompt,
} from "./loginState";
import { useLogin, type LoginState } from "./useLogin";

export type LoginDialogMode = "play" | "add" | "relogin";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const primaryButton = [
  "inline-flex h-9 items-center justify-center gap-2 rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

const secondaryButton = [
  "inline-flex h-9 items-center justify-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

// 文字だけの控えめな操作（キャンセル、別のアカウント、外部のページ）
const textButton = [
  "inline-flex items-center gap-1 rounded-sm text-sm text-text-muted underline underline-offset-2",
  "data-hovered:text-text data-pressed:text-text-strong",
  ring,
].join(" ");

type Props = {
  mode: LoginDialogMode;
  /** play と relogin で、ダイアログを開いた理由。relogin では loginRequired の accountId を対象のアカウントとして使う。 */
  prompt?: LoginPrompt | null;
  /** play で起動するインスタンス。 */
  instanceId?: string;
  isOpen?: boolean;
  onClose: () => void;
};

/**
 * ログインのダイアログ。play はプレイの失敗から開き、成功したら同じインスタンスを起動する（spec 6.3）。
 * add と relogin はアカウント画面から開き、成功したら閉じるだけにする。
 */
export function LoginDialog({ mode, prompt, instanceId, isOpen = true, onClose }: Props) {
  // ログインの途中で Esc を押して閉じたら、ログインもキャンセルする
  const cancelRef = useRef<(() => void) | null>(null);
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={(open) => {
        if (open) return;
        cancelRef.current?.();
        onClose();
      }}
      // ブラウザでの操作から戻ったときの誤操作で閉じないよう、外側を押しても閉じない（Esc では閉じる）
      isDismissable={false}
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog className="outline-hidden">
          <LoginContent mode={mode} prompt={prompt ?? null} instanceId={instanceId} onClose={onClose} cancelRef={cancelRef} />
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function titleKey(mode: LoginDialogMode, prompt: LoginPrompt | null) {
  if (mode === "add") return "login.titleAdd" as const;
  if (mode === "relogin") return "login.titleReloginAccount" as const;
  if (prompt?.kind === "offline") return "login.titleOffline" as const;
  if (prompt?.kind === "loginRequired") return "login.titleRelogin" as const;
  return "login.titleSignIn" as const;
}

/** ダイアログの中身。開くたびに作り直されるため、ログインの状態は毎回まっさらから始まる。 */
function LoginContent({
  mode,
  prompt,
  instanceId,
  onClose,
  cancelRef,
}: {
  mode: LoginDialogMode;
  prompt: LoginPrompt | null;
  instanceId: string | undefined;
  onClose: () => void;
  cancelRef: RefObject<(() => void) | null>;
}) {
  const { t } = useTranslation();
  const login = useLogin();
  const { data: accounts } = useAccounts();
  const play = usePlay(instanceId ?? "");
  // 開いたときと別のアカウントでログインした場合の、そのアカウントの ID
  const [switchedTo, setSwitchedTo] = useState<string | null>(null);

  useEffect(() => {
    cancelRef.current = login.running ? login.cancel : null;
  }, [cancelRef, login.running, login.cancel]);

  useEffect(() => {
    if (!login.result) return;
    const id = login.result.accountId;
    // アカウントがない状態からのログインは、遊ぶためのログインなので、そのまま起動する
    const expected = prompt?.kind === "loginRequired" ? prompt.accountId : null;
    if (mode === "add" || expected === null || sameAccount(expected, id)) {
      if (mode === "play" && instanceId) play.retry();
      onClose();
      return;
    }
    // 意図しないアカウントでワールドやサーバーに入らないよう、自動では起動しない（spec 4.5）
    setSwitchedTo(id);
  }, [login.result]);

  const cancel = () => {
    login.cancel();
    onClose();
  };

  const target =
    prompt?.kind === "loginRequired"
      ? accounts?.accounts.find((a) => sameAccount(a.id, prompt.accountId))
      : prompt?.kind === "offline"
        ? (() => {
            const active = pickActive(accounts);
            return active?.kind === "msa" ? active.account : undefined;
          })()
        : undefined;

  let body: ReactNode;
  if (switchedTo !== null) {
    const name = accounts?.accounts.find((a) => sameAccount(a.id, switchedTo))?.username ?? "";
    body = (
      <>
        <div className="flex items-center gap-3" role="status">
          <SkinFace accountId={switchedTo} size={32} />
          <p className="text-text-strong">{t("login.loggedInAs", { name })}</p>
        </div>
        {mode === "play" && <p className="text-sm text-text-muted">{t("login.loggedInAsHint")}</p>}
        <Footer>
          <Button autoFocus onPress={onClose} className={primaryButton}>
            {t("login.close")}
          </Button>
        </Footer>
      </>
    );
  } else if (login.running) {
    body = <ProgressView login={login} onCancel={cancel} />;
  } else if (mode === "play" && prompt?.kind === "offline") {
    body = <OfflineView play={play} offlineAvailable={prompt.offlineAvailable} account={target} onClose={onClose} onCancel={cancel} />;
  } else {
    body = (
      <>
        {target && <AccountRow id={target.id} name={target.username} />}
        {mode === "play" && <p className="text-sm text-text-muted">{t("login.launchAfter")}</p>}
        {login.error && <LoginError error={login.error} />}
        <Footer
          links={
            <>
              {mode === "play" && prompt?.kind === "loginRequired" && (
                <Link href="#/accounts" onPress={onClose} className={textButton}>
                  {t("login.useOther")}
                </Link>
              )}
              <Button onPress={cancel} className={textButton}>
                {t("login.cancel")}
              </Button>
            </>
          }
        >
          <Button autoFocus onPress={() => login.start("browser")} className={primaryButton}>
            {mode === "play" ? t("login.signInAndPlay") : t("login.signIn")}
          </Button>
        </Footer>
      </>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-5">
      <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
        {t(titleKey(mode, prompt))}
      </Heading>
      {body}
    </div>
  );
}

/** 下端の操作の並び。左に文字のリンク、右にボタン。 */
function Footer({ links, children }: { links?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
      <div className="flex items-center gap-4">{links}</div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}

function AccountRow({ id, name }: { id: string; name: string }) {
  return (
    <div className="flex items-center gap-3 rounded-md bg-surface-1 px-3 py-2">
      <SkinFace accountId={id} size={32} />
      <span className="min-w-0 truncate text-text-strong">{name}</span>
    </div>
  );
}

function OfflineView({
  play,
  offlineAvailable,
  account,
  onClose,
  onCancel,
}: {
  play: Play;
  offlineAvailable: boolean;
  account: { id: string; username: string } | undefined;
  onClose: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      {account && <AccountRow id={account.id} name={account.username} />}
      <p className="text-sm text-text-muted">{t("login.offlineBody")}</p>
      {!offlineAvailable && <p className="text-sm text-text">{t("login.offlineNeedsVerify")}</p>}
      <Footer
        links={
          <Button onPress={onCancel} className={textButton}>
            {t("login.cancel")}
          </Button>
        }
      >
        <Button
          autoFocus={!offlineAvailable}
          onPress={() => {
            play.retry();
            onClose();
          }}
          className={offlineAvailable ? secondaryButton : primaryButton}
        >
          {t("login.retry")}
        </Button>
        {offlineAvailable && (
          <Button
            autoFocus
            onPress={() => {
              play.playOffline();
              onClose();
            }}
            className={primaryButton}
          >
            {t("login.playOffline")}
          </Button>
        )}
      </Footer>
    </>
  );
}

function ProgressView({ login, onCancel }: { login: LoginState; onCancel: () => void }) {
  const { t } = useTranslation();
  const progress = login.progress;
  const cancelButton = (autoFocus: boolean) => (
    <Button autoFocus={autoFocus} onPress={onCancel} className={textButton}>
      {t("login.cancel")}
    </Button>
  );

  if (progress?.stage === "waitingForBrowser") {
    const url = progress.url;
    return (
      <>
        <Status text={t("login.waiting")} />
        <p className="text-sm text-text-muted">{t("login.waitingHint")}</p>
        <Footer links={cancelButton(false)}>
          <Button onPress={() => login.start("deviceCode")} className={secondaryButton}>
            {t("login.useCode")}
          </Button>
          <Button
            autoFocus
            isDisabled={!url.startsWith("https://")}
            onPress={() => void openExternal(url).catch((e) => console.warn("open failed", e))}
            className={secondaryButton}
          >
            <IconExternal aria-hidden className="size-4" />
            <span>{t("login.reopen")}</span>
          </Button>
        </Footer>
      </>
    );
  }
  if (progress?.stage === "deviceCode") {
    return <DeviceCodeView userCode={progress.userCode} verificationUri={progress.verificationUri} cancel={cancelButton(false)} />;
  }
  const checking = progress?.stage === "checkingXbox" || progress?.stage === "checkingMinecraft";
  return (
    <>
      <Status text={checking ? t("login.checking") : t("login.preparing")} />
      <Footer>{cancelButton(true)}</Footer>
    </>
  );
}

/** 進み具合の文。変わったら読み上げる。 */
function Status({ text }: { text: string }) {
  return (
    <p role="status" className="flex items-center gap-2 text-text-strong">
      <span aria-hidden className="size-2 shrink-0 rounded-full bg-accent motion-safe:animate-pulse" />
      <span>{text}</span>
    </p>
  );
}

function DeviceCodeView({ userCode, verificationUri, cancel }: { userCode: string; verificationUri: string; cancel: ReactNode }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const allowed = isAllowedVerificationUri(verificationUri);
  return (
    <>
      <p className="text-sm text-text-muted">{t("login.codeInstruction")}</p>
      <div className="flex items-center justify-between gap-3 rounded-md bg-surface-1 px-3 py-2">
        <span className="tabular min-w-0 select-text break-all text-sm text-text">{verificationUri}</span>
        {allowed && (
          <Button
            onPress={() => void openExternal(verificationUri).catch((e) => console.warn("open failed", e))}
            className={`${secondaryButton} shrink-0`}
          >
            <IconExternal aria-hidden className="size-4" />
            <span>{t("login.openPage")}</span>
          </Button>
        )}
      </div>
      <div className="flex items-center justify-between gap-3">
        <span className="tabular select-text text-2xl font-semibold tracking-[0.12em] text-text-strong">{userCode}</span>
        <Button
          autoFocus
          onPress={() => {
            navigator.clipboard
              ?.writeText(userCode)
              .then(() => setCopied(true))
              .catch((e) => console.warn("copy failed", e));
          }}
          className={secondaryButton}
        >
          <IconCopy aria-hidden className="size-4" />
          <span>{t("login.copy")}</span>
        </Button>
      </div>
      <p aria-live="polite" className="min-h-5 text-sm text-text-muted">
        {copied ? t("login.copied") : ""}
      </p>
      <Footer>{cancel}</Footer>
    </>
  );
}

/** ログインの失敗。spec 7 節の案内（ヒントと、固定の外部のページ）を添える。 */
function LoginError({ error }: { error: ErrorPayload }) {
  const { t } = useTranslation();
  const open = (url: string) => void openExternal(url).catch((e) => console.warn("open failed", e));
  const hint =
    error.code === "auth.app_not_approved"
      ? t("login.hint.appNotApproved")
      : error.code === "auth.denied"
        ? t("login.hint.denied")
        : null;
  const links: { label: string; url: string }[] =
    error.code === "auth.not_owned"
      ? [
          { label: t("login.buy"), url: PURCHASE_URL },
          { label: t("login.trial"), url: TRIAL_URL },
        ]
      : error.code === "auth.no_profile"
        ? [{ label: t("login.createProfile"), url: PROFILE_URL }]
        : [];
  return (
    <div role="alert" className="flex flex-col gap-2 rounded-md bg-surface-1 p-3">
      <FailureNotice error={error} />
      {hint && <p className="text-sm text-text">{hint}</p>}
      {links.length > 0 && (
        <div className="flex flex-wrap gap-4">
          {links.map((l) => (
            <Button key={l.url} onPress={() => open(l.url)} className={textButton}>
              <span>{l.label}</span>
              <IconExternal aria-hidden className="size-3.5" />
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}

// ---- プレイの失敗から開くダイアログの置き場所 ----
// ホームのプレイボタンがあればそこに置き（閉じたらフォーカスをプレイボタンに戻す）、なければコマンドパレットが置く

const hosts = new Map<string, number>();
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

/** このインスタンスのダイアログを、呼び出した部品が置くことを登録する。 */
export function usePlayDialogHost(instanceId: string): void {
  useEffect(() => {
    hosts.set(instanceId, (hosts.get(instanceId) ?? 0) + 1);
    notify();
    return () => {
      const n = (hosts.get(instanceId) ?? 1) - 1;
      if (n <= 0) hosts.delete(instanceId);
      else hosts.set(instanceId, n);
      notify();
    };
  }, [instanceId]);
}

/** このインスタンスのダイアログを置く部品があるか。 */
export function useHasPlayDialogHost(instanceId: string): boolean {
  return useSyncExternalStore(subscribe, () => (hosts.get(instanceId) ?? 0) > 0);
}

/** usePlay の prompt に合わせて開閉する、プレイのログインのダイアログ。 */
export function PlayLoginDialog({ instanceId, play, onClosed }: { instanceId: string; play: Play; onClosed?: () => void }) {
  // 閉じる動きの間も、閉じる前の中身を見せておく
  const [shown, setShown] = useState<LoginPrompt | null>(play.prompt);
  if (play.prompt !== null && play.prompt !== shown && JSON.stringify(play.prompt) !== JSON.stringify(shown)) {
    setShown(play.prompt);
  }
  return (
    <LoginDialog
      mode="play"
      prompt={play.prompt ?? shown}
      instanceId={instanceId}
      isOpen={play.prompt !== null}
      onClose={() => {
        play.dismissPrompt();
        onClosed?.();
      }}
    />
  );
}
