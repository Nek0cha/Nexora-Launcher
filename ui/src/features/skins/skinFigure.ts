import type { SkinVariant } from "../../bindings/SkinVariant";

/** PNG の data URL から高さ（IHDR）を読む。読めなければ null。同期で読めるので、描画の最初の 1 フレームから絵が出る */
export function pngHeight(dataUrl: string | null): number | null {
  if (!dataUrl) return null;
  const marker = ";base64,";
  const at = dataUrl.indexOf(marker);
  if (at < 0) return null;
  try {
    // 先頭 24 バイト（署名 8 + IHDR の長さと名前 8 + 幅 4 + 高さ 4）= base64 で 32 文字
    const bytes = atob(dataUrl.slice(at + marker.length, at + marker.length + 32));
    if (bytes.length < 24 || !bytes.startsWith("\x89PNG")) return null;
    const h = (bytes.charCodeAt(20) << 24) | (bytes.charCodeAt(21) << 16) | (bytes.charCodeAt(22) << 8) | bytes.charCodeAt(23);
    return h > 0 ? h : null;
  } catch {
    return null;
  }
}

/** 人形の 1 部位。tex は画像の中の位置と大きさ（px）、box は人形（幅 16 × 高さ 32）の中の位置と大きさ */
export type FigurePart = {
  tex: { x: number; y: number; w: number; h: number };
  box: { x: number; y: number; w: number; h: number };
  /** 左右を反転して描く（64×32 の旧形式は、左の腕と脚が右の使い回し） */
  mirror: boolean;
  /** 2 枚目の層（帽子、上着、袖、ズボン） */
  overlay: boolean;
};

/** 正面から見た人形の部位。下の層をすべて並べてから、上の層を並べる（描画の順） */
export function figureParts(height: number, variant: SkinVariant): FigurePart[] {
  const arm = variant === "slim" ? 3 : 4;
  const legacy = height < 64;
  const part = (tx: number, ty: number, tw: number, th: number, bx: number, by: number, mirror = false, overlay = false): FigurePart => ({
    tex: { x: tx, y: ty, w: tw, h: th },
    box: { x: bx, y: by, w: tw, h: th },
    mirror,
    overlay,
  });
  const base = [
    part(8, 8, 8, 8, 4, 0), // 頭
    part(20, 20, 8, 12, 4, 8), // 胴
    part(44, 20, arm, 12, 4 - arm, 8), // 右腕（見る人の左）
    legacy ? part(44, 20, arm, 12, 12, 8, true) : part(36, 52, arm, 12, 12, 8), // 左腕
    part(4, 20, 4, 12, 4, 20), // 右脚
    legacy ? part(4, 20, 4, 12, 8, 20, true) : part(20, 52, 4, 12, 8, 20), // 左脚
  ];
  // 旧形式には帽子の層しかない
  const overlay = [
    part(40, 8, 8, 8, 4, 0, false, true),
    ...(legacy
      ? []
      : [
          part(20, 36, 8, 12, 4, 8, false, true),
          part(44, 36, arm, 12, 4 - arm, 8, false, true),
          part(52, 52, arm, 12, 12, 8, false, true),
          part(4, 36, 4, 12, 4, 20, false, true),
          part(4, 52, 4, 12, 8, 20, false, true),
        ]),
  ];
  return [...base, ...overlay];
}
