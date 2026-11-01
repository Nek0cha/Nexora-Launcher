import { Button, ProgressBar } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useUpdateInstall } from "../features/update/useUpdateInstall";
import { useUpdateState } from "../features/update/updateStore";

const installButton = [
  "inline-flex h-9 items-center rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/**
 * 新しいバージョンの番号と「更新して再起動」のボタン。ダウンロード中は進み具合に変わる。
 * 更新が無いときは何も描かない。ヘッダーのポップオーバーと設定画面で共有する。
 */
export function UpdateInstall() {
  const { t } = useTranslation();
  const state = useUpdateState();
  const { canInstall, install } = useUpdateInstall();
  if (state.kind === "downloading") {
    const percent = state.percent;
    return (
      <ProgressBar
        value={percent ?? 0}
        isIndeterminate={percent === null}
        aria-label={t("update.downloading")}
        className="flex min-w-48 flex-col gap-1"
      >
        <span className="tabular text-sm text-text-muted">{percent === null ? t("update.downloading") : `${percent}%`}</span>
        <div className="relative h-1 overflow-hidden rounded-full bg-surface-3">
          <div
            className="h-full origin-left bg-accent"
            style={{ transform: `scaleX(${(percent ?? 0) / 100})` }}
          />
        </div>
      </ProgressBar>
    );
  }
  if (state.kind !== "available") return null;
  return (
    <div className="flex items-center gap-3">
      <span className="tabular text-sm text-text">{state.version}</span>
      <Button isDisabled={!canInstall} onPress={() => void install()} className={installButton}>
        {t("update.install")}
      </Button>
    </div>
  );
}
