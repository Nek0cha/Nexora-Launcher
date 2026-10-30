import { Button, ListBox, ListBoxItem, Popover, Select as AriaSelect, SelectValue } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCaret from "~icons/ph/caret-up-down";
import IconCheck from "~icons/ph/check";
import { SkinFace } from "../../features/accounts/SkinFace";
import { useAccounts } from "../../features/accounts/useAccounts";
import { optionClass, popoverMotion } from "../Select";

const FACE_SIZE = 24;

// 幅は列の幅いっぱいで固定、高さも固定。名前の長さや状態では箱の大きさを変えず、長い名前は切り詰める
const boxClass = "flex h-9 w-full min-w-0 items-center gap-2 rounded-md border border-line-control bg-surface-1 px-3 text-text";

const triggerClass = [
  boxClass,
  "text-left transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3 disabled:opacity-50",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

/** 顔と名前の 1 行。名前だけが縮む */
function Who({ id, name }: { id: string; name: string }) {
  return (
    <>
      <SkinFace accountId={id} size={FACE_SIZE} />
      <span className="min-w-0 flex-1 truncate">{name}</span>
    </>
  );
}

/**
 * スキン画面で、スキンを見る・変える相手の Microsoft アカウントを選ぶ。
 * 選べるのは Microsoft アカウントだけ（開発用のオフラインは適用も現在のスキンも持たない）。
 * 2 つ以上あるとき、または選んでいるものが無いときだけ押せる。1 つだけで選ばれているときは、同じ大きさの箱に顔と名前だけを出す。
 * disabled は、適用・マント・追加の最中（途中で別のアカウントへ移ると、進行中の印の持ち主が曖昧になるため）。
 */
export function AccountSwitch({ accountId, onChange, disabled }: {
  accountId: string | null;
  onChange: (id: string) => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const { data } = useAccounts();
  const accounts = data?.accounts ?? [];
  if (accounts.length === 0) return null;

  const selected = accounts.find((a) => a.id === accountId) ?? null;
  if (accounts.length === 1 && selected) {
    return (
      <div className={boxClass}>
        <Who id={selected.id} name={selected.username} />
      </div>
    );
  }

  return (
    <AriaSelect
      aria-label={t("nav.accounts")}
      selectedKey={selected?.id ?? null}
      onSelectionChange={(key) => {
        if (key !== null) onChange(String(key));
      }}
      isDisabled={disabled}
      className="block w-full min-w-0"
    >
      <Button className={triggerClass}>
        <SelectValue className="flex min-w-0 flex-1 items-center gap-2">
          {() => (selected ? <Who id={selected.id} name={selected.username} /> : <span />)}
        </SelectValue>
        <IconCaret aria-hidden className="size-4 shrink-0 text-text-muted" />
      </Button>
      <Popover
        offset={4}
        className={`min-w-(--trigger-width) rounded-lg border border-line bg-surface-2 p-1 shadow-float ${popoverMotion}`}
      >
        <ListBox className="scroll-thin max-h-72 overflow-auto outline-none">
          {accounts.map((a) => (
            <ListBoxItem key={a.id} id={a.id} textValue={a.username} className={optionClass}>
              {({ isSelected }) => (
                <>
                  <span className="grid size-4 shrink-0 place-items-center">
                    {isSelected && <IconCheck aria-hidden className="size-4 text-accent" />}
                  </span>
                  <Who id={a.id} name={a.username} />
                </>
              )}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </AriaSelect>
  );
}
