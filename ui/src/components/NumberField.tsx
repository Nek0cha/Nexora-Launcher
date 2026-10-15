import IconMinus from "~icons/ph/minus";
import IconPlus from "~icons/ph/plus";
import { Button, Group, Input, Label, NumberField as AriaNumberField, Text } from "react-aria-components";

// 押せる範囲は 28px 四方。押した瞬間に面を1段深くし、長押しの連続増減は React Aria の標準の動作に任せる
const stepButton = [
  "grid size-7 shrink-0 place-items-center rounded-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  "disabled:text-text-faint disabled:hover:bg-transparent",
].join(" ");

export function NumberField(props: {
  label: string;
  description?: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
}) {
  return (
    <AriaNumberField
      value={props.value}
      minValue={props.min}
      maxValue={props.max}
      step={1}
      formatOptions={{ maximumFractionDigits: 0 }}
      // スクロールの途中で値が変わる事故を防ぐ
      isWheelDisabled
      onChange={(v) => {
        if (Number.isInteger(v)) props.onChange(v);
      }}
      className="flex items-center justify-between gap-4"
    >
      <div className="flex flex-col">
        <Label className="text-text">{props.label}</Label>
        {props.description && (
          <Text slot="description" className="text-xs text-text-muted">
            {props.description}
          </Text>
        )}
      </div>
      {/* フォーカスリングは入力欄ではなく、それを囲む Group に付ける */}
      <Group className="flex h-9 shrink-0 items-center gap-1 rounded-md border border-line-control bg-surface-1 px-1 focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-accent">
        <Button slot="decrement" className={stepButton}>
          <IconMinus aria-hidden className="size-4" />
        </Button>
        <Input className="tabular w-10 min-w-0 bg-transparent text-center text-text outline-none" />
        <Button slot="increment" className={stepButton}>
          <IconPlus aria-hidden className="size-4" />
        </Button>
      </Group>
    </AriaNumberField>
  );
}
