import { useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import IconShirt from "~icons/ph/t-shirt";
import { EmptyState } from "../components/EmptyState";
import { ReadyGate, useLatchedPending } from "../components/ReadyGate";
import { Skeleton } from "../components/Skeleton";
import { AccountSwitch } from "../components/skins/AccountSwitch";
import { CapePicker } from "../components/skins/CapePicker";
import { DefaultSkins } from "../components/skins/DefaultSkins";
import { SavedSkins } from "../components/skins/SavedSkins";
import { SkinPreviewSlot } from "../components/skins/SkinPreviewSlot";
import { useAccounts } from "../features/accounts/useAccounts";
import { LoginDialog, type LoginDialogMode } from "../features/login/LoginDialog";
import type { LoginPrompt } from "../features/login/loginState";
import { canApply } from "../features/skins/canApply";
import { useCurrentSkin, useSavedSkins } from "../features/skins/useSkins";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 空の状態の主な操作。アカウント画面と同じ明暗の反転の塗り
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

const sectionHeading = "font-display text-lg font-semibold text-text-strong";

// 見出しの行の高さ。再ログインのボタン（h-9）が出入りしても、下の中身が動かないよう固定する
const headerRow = "flex h-9 items-center justify-between gap-4";

/** ダイアログの開閉と、開いた時点の中身。閉じる動きの間に中身が変わらないよう、開くときに決めて持つ */
type LoginState = { open: boolean; mode: LoginDialogMode; prompt: LoginPrompt | null };

/** スキン画面の枠。左にプレビューとアカウントの切り替え、右に保存したスキンとデフォルトのスキン。中の部品は後のタスクで差し込む。 */
export default function Skins() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const saved = useSavedSkins();
  // 選んでいるアカウント。未選択の間は使用中の msa アカウント
  const [picked, setPicked] = useState<string | null>(null);
  const [login, setLogin] = useState<LoginState>({ open: false, mode: "add", prompt: null });
  // 適用・マント・追加の最中か（SavedSkins が伝える）。この間はアカウントを切り替えさせない
  const [savedBusy, setSavedBusy] = useState(false);
  const [defaultBusy, setDefaultBusy] = useState(false);
  const [capeBusy, setCapeBusy] = useState(false);
  const busy = savedBusy || defaultBusy || capeBusy;

  const list = accounts.data;
  // 選んだアカウントが一覧から消えたら、使用中の msa アカウントに戻る。以降の判断はすべてこの有効なアカウントで行う
  const activeMsa = accounts.active?.kind === "msa" ? accounts.active.account.id : null;
  const accountId = picked !== null && list?.accounts.some((a) => a.id === picked) ? picked : activeMsa;
  const selected = list?.accounts.find((a) => a.id === accountId) ?? null;
  const needsLogin = selected?.needsLogin ?? false;
  // 適用できるのは、ログインが有効な Microsoft アカウントのときだけ（開発用のオフラインは accountId が null）
  const applicable = selected ? canApply({ kind: "msa", account: selected }) : false;
  // 適用できないアカウントの現在のスキンは取りにいかない（プレビューは空の枠のまま）
  const current = useCurrentSkin(applicable ? accountId : null).data;
  const capeImage = current?.capes.find((c) => c.active)?.image ?? null;

  // 最初の取得がそろうまでスケルトン。そろったあとの取り直しでは枠に戻らない
  const pending = useLatchedPending(!list && !accounts.error || saved.isPending);

  // 本物の枠と同じ並び・高さ（見出しの行と gap-6、見出しの行の高さ）にして、中身が来ても下がらないようにする
  const skeleton = (
    <div data-testid="skins-skeleton" className="flex flex-col gap-6 p-10">
      <div className={headerRow}>
        <Skeleton className="h-7 w-24" />
      </div>
      <div className="flex gap-10">
        <Skeleton className="h-96 w-64 shrink-0" />
        <div className="flex min-w-0 flex-1 flex-col gap-8">
          <div className="flex flex-col gap-3">
            <Skeleton className="h-6 w-32" />
            <Skeleton className="h-40 w-full" />
          </div>
          <div className="flex flex-col gap-3">
            <Skeleton className="h-6 w-40" />
            <Skeleton className="h-40 w-full" />
          </div>
        </div>
      </div>
    </div>
  );

  const openLogin = (mode: LoginDialogMode, prompt: LoginPrompt | null) => setLogin({ open: true, mode, prompt });
  const loginDialog = (
    // 閉じると LoginDialog の中身が外れ、動いているログインはキャンセルされる
    <LoginDialog
      mode={login.mode}
      prompt={login.prompt}
      isOpen={login.open}
      onClose={() => setLogin((l) => ({ ...l, open: false }))}
    />
  );

  const noAccounts = !!list && list.accounts.length === 0 && list.devOffline === null;

  return (
    <>
      <ReadyGate queries={[{ isPending: pending }]} skeleton={skeleton}>
        {noAccounts ? (
          <EmptyState icon={IconShirt} title={t("nav.skins")}>
            <Button onPress={() => openLogin("add", null)} className={primaryButton}>
              <IconPlus aria-hidden className="size-5" />
              <span>{t("accounts.add")}</span>
            </Button>
          </EmptyState>
        ) : (
          <div className="flex flex-col gap-6 p-10">
            <div className={headerRow}>
              <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("nav.skins")}</h1>
              {needsLogin && selected && (
                <Button
                  onPress={() => openLogin("relogin", { kind: "loginRequired", accountId: selected.id })}
                  className={secondaryButton}
                >
                  {t("accounts.relogin")}
                </Button>
              )}
            </div>
            <div className="flex gap-10">
              {/* 左の列。3D プレビュー（大きさは読み込み中と同じ）と、その下にアカウントの切り替え */}
              <div className="flex w-64 shrink-0 flex-col gap-3">
                <SkinPreviewSlot image={current?.image ?? null} variant={current?.variant ?? "classic"} capeImage={capeImage} />
                <CapePicker accountId={accountId} canApply={applicable} blocked={savedBusy || defaultBusy} onBusyChange={setCapeBusy} />
                <AccountSwitch accountId={accountId} onChange={setPicked} disabled={busy} />
              </div>
              <div className="flex min-w-0 flex-1 flex-col gap-8">
                <section className="flex flex-col gap-3">
                  <h2 className={sectionHeading}>{t("skins.saved")}</h2>
                  <SavedSkins accountId={accountId} canApply={applicable} blocked={defaultBusy || capeBusy} onBusyChange={setSavedBusy} />
                </section>
                <section className="flex flex-col gap-3">
                  <h2 className={sectionHeading}>{t("skins.defaults")}</h2>
                  <DefaultSkins accountId={accountId} canApply={applicable} blocked={savedBusy || capeBusy} onBusyChange={setDefaultBusy} />
                </section>
              </div>
            </div>
          </div>
        )}
      </ReadyGate>
      {loginDialog}
    </>
  );
}
