import { useTranslation } from "react-i18next";
import IconEmpty from "~icons/ph/user-circle";
import { EmptyState } from "../components/EmptyState";

export default function Accounts() {
  const { t } = useTranslation();
  return <EmptyState icon={IconEmpty} title={t("nav.accounts")} body={t("empty.accounts")} />;
}
