import { useLayoutEffect, useRef } from "react";
import { Label, Slider, SliderThumb, SliderTrack } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { toSliderValue } from "../features/instances/memorySlider";

const FORMAT: Intl.NumberFormatOptions = { style: "unit", unit: "megabyte" };

// つまみは 16px の円。軌道は細く、塗りはアクセントの色にする
export function MemorySlider(props: {
  label: string;
  /** フォームの本当の値。スライダーの右端を超えていてもそのまま渡す */
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
}) {
  const { t, i18n } = useTranslation();
  const inputRef = useRef<HTMLInputElement>(null);
  const pinned = props.value > props.max;
  const shown = new Intl.NumberFormat(i18n.language, FORMAT).format(props.value);
  // 右端を超える値は、つまみを右端に置くだけで、表示と読み上げは本当の値にする
  const valueText = pinned ? t("instances.detail.memoryAboveRange", { value: shown }) : shown;

  // React Aria のつまみは、読み上げの文を位置の値から作るため、ここで本当の値に差し替える。
  // 毎回の描画のあとに当て直す（つまみの位置が変わると React Aria が書き戻すため）。
  useLayoutEffect(() => {
    inputRef.current?.setAttribute("aria-valuetext", valueText);
  });

  return (
    <Slider
      value={toSliderValue(props.value, props.max)}
      minValue={props.min}
      maxValue={props.max}
      step={props.step}
      onChange={props.onChange}
      formatOptions={FORMAT}
      className="flex flex-col gap-1"
    >
      <div className="flex items-center justify-between gap-4">
        <Label className="text-sm text-text-muted">{props.label}</Label>
        {/* 読み上げはつまみが受け持つので、ここは見た目だけにする */}
        <span aria-hidden data-testid="memory-slider-output" className="tabular text-sm text-text">
          {shown}
        </span>
      </div>
      <SliderTrack className="relative h-5 w-full">
        {({ state }) => (
          <>
            <div className="absolute top-1/2 h-1 w-full -translate-y-1/2 rounded-full bg-surface-3" />
            <div
              className="absolute top-1/2 h-1 -translate-y-1/2 rounded-full bg-accent"
              style={{ width: `${state.getThumbPercent(0) * 100}%` }}
            />
            {/* React Aria はつまみを left と transform: translate(-50%, -50%) で置く。個別の scale はその transform より外側に掛かり、
                つまみ自身の中心ではなく元の箱の中心を基準に拡大するため、左上へずれる。拡大は内側の円で行う */}
            <SliderThumb
              inputRef={inputRef}
              className={[
                "group top-1/2 size-4 rounded-full data-hovered:opacity-90",
                "data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-accent",
              ].join(" ")}
            >
              <span
                aria-hidden
                data-testid="memory-slider-disc"
                className={[
                  "block size-full rounded-full border border-line-control bg-text-strong",
                  "transition-[scale] duration-(--duration-fast) ease-out-quint motion-reduce:transition-none",
                  "group-data-hovered:scale-110 group-data-focus-visible:scale-110 group-data-dragging:scale-110",
                ].join(" ")}
              />
            </SliderThumb>
          </>
        )}
      </SliderTrack>
    </Slider>
  );
}
