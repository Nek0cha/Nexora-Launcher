import { useMemo } from "react";
import { Button, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCaret from "~icons/ph/caret-down";
import IconPlus from "~icons/ph/plus";
import type { Instance } from "../../bindings/Instance";
import type { SearchHit } from "../../bindings/SearchHit";
import { compatibleInstances } from "../../features/modrinth/compatibleInstances";
import { StableLabel } from "../StableLabel";
import { actionButton } from "./HitCard";

// メニューの開閉。ボタンの側から 4px 動きながら現れる（インスタンスの一覧の「その他」と同じ）
const popoverClass = [
  "min-w-44 max-w-72 rounded-lg border border-line bg-surface-2 p-1 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const menuItemClass = [
  "flex h-9 cursor-default items-center gap-3 rounded-sm px-2 text-sm text-text outline-none",
  "hover:bg-surface-3 focused:bg-surface-3 disabled:text-text-muted disabled:hover:bg-transparent",
].join(" ");

const NONE = "__none";

/**
 * 全体の画面の、1 件の結果の追加の操作。「インスタンスに追加」（合うインスタンスだけを並べたメニュー）と、
 * 「最後に追加したインスタンスに追加」。後者は、未設定のときと、そのインスタンスに合わないときは押せない。
 */
export function AddToInstanceMenu(props: {
  hit: SearchHit;
  instances: Instance[];
  /** 最後に追加したインスタンス。未設定か、もう無ければ null */
  lastAdded: Instance | null;
  pending: boolean;
  onAdd: (target: Instance) => void;
}) {
  const { t } = useTranslation();
  const { hit, lastAdded } = props;
  const compatible = useMemo(() => compatibleInstances(hit, props.instances), [hit, props.instances]);
  const lastCompatible = lastAdded !== null && compatible.some((i) => i.id === lastAdded.id);

  return (
    <>
      <MenuTrigger>
        <Button isDisabled={props.pending} className={actionButton}>
          <span>{t("modrinth.addTo")}</span>
          <IconCaret aria-hidden className="size-3.5 text-text-muted" />
        </Button>
        <Popover placement="bottom end" offset={4} className={popoverClass}>
          <Menu
            aria-label={t("modrinth.chooseInstance")}
            disabledKeys={compatible.length === 0 ? [NONE] : []}
            onAction={(key) => {
              const target = compatible.find((i) => i.id === key);
              if (target) props.onAdd(target);
            }}
            className="scroll-thin max-h-72 overflow-auto outline-none"
          >
            {compatible.length === 0 ? (
              <MenuItem id={NONE} className={menuItemClass}>
                {t("modrinth.noInstances")}
              </MenuItem>
            ) : (
              compatible.map((i) => (
                <MenuItem key={i.id} id={i.id} textValue={i.name} className={menuItemClass}>
                  <span className="min-w-0 flex-1 truncate">{i.name}</span>
                </MenuItem>
              ))
            )}
          </Menu>
        </Popover>
      </MenuTrigger>
      <Button
        isDisabled={!lastCompatible || props.pending}
        onPress={() => {
          if (lastAdded) props.onAdd(lastAdded);
        }}
        className={actionButton}
      >
        <IconPlus aria-hidden className="size-3.5 shrink-0" />
        <StableLabel
          labels={[
            t("modrinth.installing"),
            lastAdded ? t("modrinth.addToLast", { name: lastAdded.name }) : t("modrinth.addToLastUnset"),
          ]}
          active={props.pending ? 0 : 1}
          className="max-w-40 truncate"
        />
      </Button>
    </>
  );
}
