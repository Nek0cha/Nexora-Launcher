import { useTranslation } from "react-i18next";

/** ロゴの形（favicon と同じ）。色はテーマのトークンに合わせる。 */
export function Logo() {
  return (
    <svg aria-hidden viewBox="0 0 32 32" className="size-5 shrink-0">
      <rect x="3.5" y="6.5" width="25" height="19" rx="4.5" fill="none" strokeWidth="3" className="stroke-text-strong" />
      <rect x="7" y="10" width="12" height="12" rx="1.5" className="fill-accent" />
    </svg>
  );
}

/**
 * ロゴとアプリ名。ロゴの左端はサイドバーのアイコン、名前の左端はラベルに揃える（余白 24px、アイコン 20px、gap 10px）。
 * アプリのバーと起動中のバーで同じものを使い、起動から画面が出るまでに位置が動かないようにする。
 */
export function Brand() {
  const { t } = useTranslation();
  return (
    <span data-testid="brand" className="mr-2 flex shrink-0 items-center gap-2.5">
      <Logo />
      <span className="font-display text-sm font-semibold tracking-[-0.01em] text-text-strong">{t("appName")}</span>
    </span>
  );
}
