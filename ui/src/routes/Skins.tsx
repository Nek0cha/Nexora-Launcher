import { lazy, Suspense, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import IconShirt from "~icons/ph/t-shirt";
import { EmptyState } from "../components/EmptyState";
import { ReadyGate, useLatchedPending } from "../components/ReadyGate";
import { Skeleton } from "../components/Skeleton";
import { SavedSkins } from "../components/skins/SavedSkins";
import { useAccounts } from "../features/accounts/useAccounts";
import { LoginDialog } from "../features/login/LoginDialog";
import { canApply } from "../features/skins/canApply";
import { useCurrentSkin, useSavedSkins } from "../features/skins/useSkins";

// 3D のライブラリは大きい（約 130 kB gzip）ので、画面を開いたあとに別の塊として読む
const SkinPreview = lazy(() => import("../components/skins/SkinPreview").then((m) => ({ default: m.SkinPreview })));

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

/** スキン画面の枠。左にプレビュー、右に保存したスキンとデフォルトのスキン。中の部品は後のタスクで差し込む。 */
export default function Skins() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const saved = useSavedSkins();
  // 選んでいるアカウント。未選択の間は使用中の msa アカウント（アカウントの切り替えは後のタスクが setAccountId で行う）
  const [picked, setAccountId] = useState<string | null>(null);
  void setAccountId;
  const [loginOpen, setLoginOpen] = useState(false);

  const accountId = picked ?? (accounts.active?.kind === "msa" ? accounts.active.account.id : null);
  const list = accounts.data;
  const selected = list?.accounts.find((a) => a.id === accountId) ?? null;
  const needsLogin = selected?.needsLogin ?? false;
  // 適用できるのは、ログインが有効な Microsoft アカウントのときだけ（開発用のオフラインは accountId が null）
  const applicable = selected ? canApply({ kind: "msa", account: selected }) : false;
  const current = useCurrentSkin(accountId).data;
  const capeImage = current?.capes.find((c) => c.active)?.image ?? null;

  // 最初の取得がそろうまでスケルトン。そろったあとの取り直しでは枠に戻らない
  const pending = useLatchedPending(!list && !accounts.error || saved.isPending);

  const skeleton = (
    <div className="flex gap-10 p-10">
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
  );

  const loginDialog = (
    // 閉じると LoginDialog の中身が外れ、動いているログインはキャンセルされる
    <LoginDialog
      mode={needsLogin ? "relogin" : "add"}
      prompt={needsLogin && selected ? { kind: "loginRequired", accountId: selected.id } : null}
      isOpen={loginOpen}
      onClose={() => setLoginOpen(false)}
    />
  );

  const noAccounts = !!list && list.accounts.length === 0 && list.devOffline === null;

  return (
    <>
      <ReadyGate queries={[{ isPending: pending }]} skeleton={skeleton}>
        {noAccounts ? (
          <EmptyState icon={IconShirt} title={t("nav.skins")}>
            <Button onPress={() => setLoginOpen(true)} className={primaryButton}>
              <IconPlus aria-hidden className="size-5" />
              <span>{t("accounts.add")}</span>
            </Button>
          </EmptyState>
        ) : (
          <div className="flex flex-col gap-6 p-10">
            <div className="flex items-center justify-between gap-4">
              <h1 className="font-display text-xl font-semibold tracking-[-0.01em] text-text-strong">{t("nav.skins")}</h1>
              {needsLogin && (
                <Button onPress={() => setLoginOpen(true)} className={secondaryButton}>
                  {t("accounts.relogin")}
                </Button>
              )}
            </div>
            <div className="flex gap-10">
              {/* 左の列。3D プレビュー（大きさは読み込み中と同じ）。アカウントの切り替えは後のタスクが足す */}
              <div className="w-64 shrink-0">
                <div data-testid="skin-preview-slot" className="h-96 w-64 overflow-hidden rounded-lg bg-surface-1">
                  <Suspense fallback={<div className="size-full" />}>
                    <SkinPreview image={current?.image ?? null} variant={current?.variant ?? "classic"} capeImage={capeImage} />
                  </Suspense>
                </div>
              </div>
              <div className="flex min-w-0 flex-1 flex-col gap-8">
                <section className="flex flex-col gap-3">
                  <h2 className={sectionHeading}>{t("skins.saved")}</h2>
                  <SavedSkins accountId={accountId} canApply={applicable} />
                </section>
                <section className="flex flex-col gap-3">
                  <h2 className={sectionHeading}>{t("skins.defaults")}</h2>
                  <div data-testid="default-skins-slot" />
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
