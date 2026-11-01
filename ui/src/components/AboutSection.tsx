import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { appInfo } from "../api/commands";
import { openExternal } from "../api/opener";
import { checkForUpdate, useUpdateState } from "../features/update/updateStore";
import { RELEASES_URL, REPO_URL } from "../features/update/repo";
import { LicensesDialog } from "./LicensesDialog";
import { UpdateInstall } from "./UpdateInstall";
import { rowLabel } from "./settingsLayout";

const rowClass = "flex items-center justify-between gap-4";

const actionButton = [
  "inline-flex h-9 shrink-0 items-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:bg-surface-3 data-pressed:bg-surface-3 data-disabled:opacity-50",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/** 設定の「このアプリについて」の行。バージョン、更新の確認、ライセンス、リポジトリへのリンク。 */
export function AboutRows() {
  const { t } = useTranslation("settings");
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: appInfo });
  const update = useUpdateState();
  const [licensesOpen, setLicensesOpen] = useState(false);
  if (!info) return null;

  const status =
    update.kind === "checking"
      ? t("about.checking")
      : update.kind === "upToDate"
        ? t("about.upToDate")
        : update.kind === "failed"
          ? t("about.checkFailed")
          : null;

  return (
    <>
      <div className={rowClass}>
        <span className={rowLabel}>{t("about.version")}</span>
        <span className="tabular text-text-muted">{info.version}</span>
      </div>
      <div className={rowClass}>
        <span className={rowLabel}>{t("about.updates")}</span>
        {info.selfUpdate ? (
          <div className="flex items-center gap-3">
            {status && (
              <span role="status" className="text-sm text-text-muted">
                {status}
              </span>
            )}
            <UpdateInstall />
            <Button
              isDisabled={update.kind === "checking" || update.kind === "downloading"}
              onPress={() => void checkForUpdate({ manual: true })}
              className={actionButton}
            >
              {t("about.check")}
            </Button>
          </div>
        ) : (
          <Button onPress={() => void openExternal(RELEASES_URL)} className={actionButton}>
            {t("about.releases")}
          </Button>
        )}
      </div>
      <div className={rowClass}>
        <span className={rowLabel}>{t("about.license")}</span>
        <span className="text-text-muted">Apache-2.0</span>
      </div>
      <div className={rowClass}>
        <span className={rowLabel}>{t("about.thirdParty")}</span>
        <Button onPress={() => setLicensesOpen(true)} className={actionButton}>
          {t("about.open")}
        </Button>
        <LicensesDialog isOpen={licensesOpen} onOpenChange={setLicensesOpen} />
      </div>
      <div className={rowClass}>
        <span className={rowLabel}>{t("about.repository")}</span>
        <Button onPress={() => void openExternal(REPO_URL)} className={actionButton}>
          GitHub
        </Button>
      </div>
    </>
  );
}
