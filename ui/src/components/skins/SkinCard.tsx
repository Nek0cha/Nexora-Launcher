import type { CSSProperties, ReactNode } from "react";
import { Button } from "react-aria-components";
import IconCheck from "~icons/ph/check-circle-fill";
import IconSpinner from "~icons/ph/circle-notch";
import type { SkinVariant } from "../../bindings/SkinVariant";
import { figureParts, type FigurePart, pngHeight } from "../../features/skins/skinFigure";

const FIGURE_W = 16;
const FIGURE_H = 32;
const BLEED = 0.5;

const pct = (n: number, d: number) => `${(n / d) * 100}%`;

/** 部位 1 つの CSS。画像全体を、部位が画像の 1 区画を埋める倍率に拡大し、その区画が見える位置に置く */
function partStyle(part: FigurePart, image: string, imageHeight: number): CSSProperties {
  const { tex, box } = part;
  // 端数の位置で隣り合う部位の間に細い隙間が出ないよう、四方に 0.25px ずつはみ出させる
  return {
    position: "absolute",
    left: `calc(${pct(box.x, FIGURE_W)} - ${BLEED / 2}px)`,
    top: `calc(${pct(box.y, FIGURE_H)} - ${BLEED / 2}px)`,
    width: `calc(${pct(box.w, FIGURE_W)} + ${BLEED}px)`,
    height: `calc(${pct(box.h, FIGURE_H)} + ${BLEED}px)`,
    backgroundImage: `url("${image}")`,
    backgroundSize: `${(64 / tex.w) * 100}% ${(imageHeight / tex.h) * 100}%`,
    backgroundPosition: `${(tex.x / (64 - tex.w)) * 100}% ${(tex.y / (imageHeight - tex.h)) * 100}%`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
    transform: part.mirror ? "scaleX(-1)" : undefined,
  };
}

/** 正面から見た人形（頭、胴、腕、脚と、あれば 2 枚目の層）を、CSS だけで切り出して描く */
function Doll({ image, variant }: { image: string; variant: SkinVariant }) {
  const height = pngHeight(image);
  if (height === null) return null;
  return (
    <span aria-hidden className="relative block h-full" style={{ aspectRatio: `${FIGURE_W} / ${FIGURE_H}` }}>
      {figureParts(height, variant).map((part, i) => (
        <span key={i} data-testid="skin-card-part" style={partStyle(part, image, height)} />
      ))}
    </span>
  );
}

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const cardButton = [
  "absolute inset-0 flex items-center justify-center rounded-lg border-2 bg-surface-1 p-3",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  ring,
].join(" ");

/**
 * 保存したスキン 1 枚のカード（縦横比 2:3）。名前は aria-label だけで、画面には出さない。
 * 枠の太さと大きさは状態で変えない（選択中は枠の色とチェックで示す）。
 * メニューは押せるボタンの外に重ねる（ボタンの中にボタンは置けない）。
 */
export function SkinCard({ image, name, variant = "classic", selected, pending, disabled, onPress, menu }: {
  image: string | null;
  name: string;
  variant?: SkinVariant;
  selected: boolean;
  /** 適用の最中のカード。進行中の印を出す */
  pending: boolean;
  disabled: boolean;
  onPress: () => void;
  menu?: ReactNode;
}) {
  return (
    // aria-busy は react-aria の Button が DOM に通さないので、カードの枠に付ける
    <div aria-busy={pending || undefined} className="relative aspect-[2/3] w-full">
      <Button
        aria-label={name}
        aria-pressed={selected}
        data-selected={selected || undefined}
        isDisabled={disabled}
        onPress={onPress}
        className={`${cardButton} ${selected ? "border-accent" : "border-line hover:border-line-strong"}`}
      >
        {image && <Doll image={image} variant={variant} />}
      </Button>
      {/* 印は右上に重ねる。どの状態でも場所だけを取り、カードの大きさを変えない */}
      {selected ? (
        <IconCheck aria-hidden data-testid="skin-card-check" className="pointer-events-none absolute top-1.5 right-1.5 size-5 text-accent" />
      ) : pending ? (
        <IconSpinner
          aria-hidden
          data-testid="skin-card-progress"
          className="pointer-events-none absolute top-1.5 right-1.5 size-5 text-text-muted motion-safe:animate-spin"
        />
      ) : null}
      {menu && <div className="absolute top-1.5 left-1.5">{menu}</div>}
    </div>
  );
}
