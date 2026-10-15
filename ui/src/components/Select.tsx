import IconCaret from "~icons/ph/caret-up-down";
import IconCheck from "~icons/ph/check";
import { Button, Label, ListBox, ListBoxItem, Popover, Select as AriaSelect, SelectValue } from "react-aria-components";

export type Option<T extends string> = { id: T; label: string };

// 一覧の開閉。ボタンの側から 4px 動きながら現れる。開くときは base、閉じるときは fast。
// 動きを減らす設定では移動をやめ、不透明度だけを fast で変える
const popoverMotion = [
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const triggerClass = [
  "flex h-9 min-w-48 select-none items-center justify-between gap-2 rounded-md border border-line-control bg-surface-1 px-3 text-left text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

// キーボードで移動中の項目は、ホバーと同じ面の明度で示す。この面がフォーカスの印の代わりになる。
// キーボードを使った後はホバーでフォーカスが移らないため、ホバーにも同じ面を直接付ける
const optionClass =
  "flex h-9 cursor-default select-none items-center gap-2 rounded-sm px-2 text-text outline-none hover:bg-surface-3 focused:bg-surface-3 selected:text-text-strong";

export function Select<T extends string>(props: {
  label: string;
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <AriaSelect
      value={props.value}
      onChange={(key) => {
        if (key !== null) props.onChange(key as T);
      }}
      className="flex items-center justify-between gap-4"
    >
      <Label className="select-none text-text">{props.label}</Label>
      <Button className={triggerClass}>
        {/* 項目の中身（チェックの場所）ごと写さず、文字だけを表示する */}
        <SelectValue className="truncate">{({ selectedText }) => selectedText}</SelectValue>
        <IconCaret aria-hidden className="size-4 shrink-0 text-text-muted" />
      </Button>
      <Popover
        offset={4}
        className={`min-w-(--trigger-width) rounded-lg border border-line bg-surface-2 p-1 shadow-float ${popoverMotion}`}
      >
        <ListBox className="max-h-72 overflow-auto outline-none">
          {props.options.map((o) => (
            <ListBoxItem key={o.id} id={o.id} textValue={o.label} className={optionClass}>
              {({ isSelected }) => (
                <>
                  {/* 印の場所は選択されていない項目でも確保し、文字の位置を揃える */}
                  <span className="grid size-4 shrink-0 place-items-center">
                    {isSelected && <IconCheck aria-hidden className="size-4 text-accent" />}
                  </span>
                  <span className="truncate">{o.label}</span>
                </>
              )}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </AriaSelect>
  );
}
