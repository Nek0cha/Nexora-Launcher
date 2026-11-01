import { Button, Dialog, DialogTrigger, Popover } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconArrowUp from "~icons/ph/arrow-circle-up";
import { useUpdateState } from "../../features/update/updateStore";
import { UpdateInstall } from "../UpdateInstall";

const popoverClass = [
  "rounded-lg border border-line bg-surface-2 p-3 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0 data-exiting:duration-(--duration-fast)",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

/** 新しいバージョンがあるときだけ出る、ヘッダーの更新ボタン。押すと、バージョンと更新のボタンを開く。 */
export function UpdateButton() {
  const { t } = useTranslation();
  const state = useUpdateState();
  if (state.kind !== "available" && state.kind !== "downloading") return null;
  return (
    <DialogTrigger>
      <Button
        aria-label={t("update.available")}
        className={[
          "relative grid size-8 shrink-0 place-items-center rounded-full text-accent",
          "transition-colors duration-(--duration-fast) ease-out-quint data-hovered:bg-surface-2 data-pressed:bg-surface-3",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        ].join(" ")}
      >
        <IconArrowUp aria-hidden className="size-4" />
      </Button>
      <Popover placement="bottom end" offset={6} className={popoverClass}>
        <Dialog aria-label={t("update.available")} className="outline-hidden">
          <UpdateInstall />
        </Dialog>
      </Popover>
    </DialogTrigger>
  );
}
