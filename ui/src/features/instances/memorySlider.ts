import { MEMORY_MAX_MB, MEMORY_MIN_MB } from "./settingsForm";

/** スライダーの刻み（MB）。 */
export const SLIDER_STEP_MB = 256;

/** スライダーの右端。物理メモリを刻みに切り捨て、手動で指定できる範囲に収める。 */
export function sliderMax(totalMb: number): number {
  const floored = Math.floor(totalMb / SLIDER_STEP_MB) * SLIDER_STEP_MB;
  return Math.min(MEMORY_MAX_MB, Math.max(MEMORY_MIN_MB, floored));
}

/** スライダーに置く位置。右端を超える値は右端に置く（フォームの値は変えない）。 */
export function toSliderValue(memoryMb: number, max: number): number {
  return Math.min(memoryMb, max);
}
