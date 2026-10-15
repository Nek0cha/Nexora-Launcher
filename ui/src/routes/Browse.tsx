import { useTranslation } from "react-i18next";
import IconEmpty from "~icons/ph/compass";
import { EmptyState } from "../components/EmptyState";

export default function Browse() {
  const { t } = useTranslation();
  return <EmptyState icon={IconEmpty} title={t("nav.browse")} body={t("empty.browse")} />;
}
