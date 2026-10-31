import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconImport from "~icons/ph/download-simple";
import { pickMrpack } from "../../api/dialog";
import { openMrpackImport } from "../../features/mrpack/useMrpackImport";
import { useToast } from "../toast/useToast";
import { quietTrigger } from "./dialogStyles";

/** `.mrpack` を選んで、読み込みの確認ダイアログを開く。 */
export function ImportMrpackButton() {
  const { t } = useTranslation();
  const toast = useToast();
  return (
    <Button
      className={quietTrigger}
      onPress={() => {
        pickMrpack()
          .then((path) => {
            if (path !== null) openMrpackImport(path);
          })
          .catch(toast.showError);
      }}
    >
      <IconImport aria-hidden className="size-4" />
      <span>{t("mrpack.import.open")}</span>
    </Button>
  );
}
