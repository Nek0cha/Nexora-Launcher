import { Label, Slider, SliderOutput, SliderThumb, SliderTrack } from "react-aria-components";

// つまみは 16px の円。軌道は細く、塗りはアクセントの色にする
export function MemorySlider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
}) {
  return (
    <Slider
      value={props.value}
      minValue={props.min}
      maxValue={props.max}
      step={props.step}
      onChange={props.onChange}
      formatOptions={{ style: "unit", unit: "megabyte" }}
      className="flex flex-col gap-1"
    >
      <div className="flex items-center justify-between gap-4">
        <Label className="text-sm text-text-muted">{props.label}</Label>
        <SliderOutput className="tabular text-sm text-text" />
      </div>
      <SliderTrack className="relative h-5 w-full">
        {({ state }) => (
          <>
            <div className="absolute top-1/2 h-1 w-full -translate-y-1/2 rounded-full bg-surface-3" />
            <div
              className="absolute top-1/2 h-1 -translate-y-1/2 rounded-full bg-accent"
              style={{ width: `${state.getThumbPercent(0) * 100}%` }}
            />
            <SliderThumb
              className={[
                "top-1/2 size-4 rounded-full border border-line-control bg-text-strong",
                "data-dragging:scale-110 data-hovered:opacity-90",
                "data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-accent",
              ].join(" ")}
            />
          </>
        )}
      </SliderTrack>
    </Slider>
  );
}
