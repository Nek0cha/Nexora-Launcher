import { useTranslation } from "react-i18next";
import IconEmpty from "~icons/ph/stack";
import { EmptyState } from "../components/EmptyState";

export default function Instances() {
  const { t } = useTranslation();
  return <EmptyState icon={IconEmpty} title={t("nav.instances")} body={t("empty.instances")} />;
}
