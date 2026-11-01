import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconImport from "~icons/ph/arrows-left-right";
import { openLauncherImport } from "../../features/import/useLauncherImport";
import { quietTrigger } from "../mrpack/dialogStyles";

/** 他のランチャーから読み込むダイアログを開く。 */
export function ImportLauncherButton() {
  const { t } = useTranslation();
  return (
    <Button className={quietTrigger} onPress={openLauncherImport}>
      <IconImport aria-hidden className="size-4" />
      <span>{t("launcherImport.open")}</span>
    </Button>
  );
}
