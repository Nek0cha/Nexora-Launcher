/**
 * 配置の安定の規則を確かめるためのテスト用の道具。
 * 状態（選択、押下、無効など）の接頭辞が付いたクラスのうち、寸法を変えるもの
 * （枠線の太さ、文字の太さや大きさ、余白、幅、高さ）を取り出す。状態は色、背景、影、輪郭（ring、outline）だけで示す。
 */
const stateVariant =
  /^(?:selected|pressed|hover|hovered|focus|focused|focus-visible|active|open|pending|invalid|expanded|checked|disabled|dragging|drop-target|data-.+|aria-.+|group-.+|peer-.+)$/;

const sizeUtility =
  /^-?(?:border(?:-[xytrblse])?(?:-\d+|-\[[^\]]+\])?|font-(?:thin|extralight|light|normal|medium|semibold|bold|extrabold|black)|p[xytrblse]?-.+|m[xytrblse]?-.+|w-.+|h-.+|size-.+|min-[wh]-.+|max-[wh]-.+|gap(?:-[xy])?-.+|text-(?:xs|sm|base|lg|\d?xl)|tracking-.+|leading-.+)$/;

/** `group-data-[x:y]:border-2` のような、括弧の中に : を含む接頭辞も切り分ける */
function splitVariants(token: string): { variants: string[]; base: string } {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < token.length; i++) {
    const c = token[i];
    if (c === "[") depth++;
    else if (c === "]") depth--;
    else if (c === ":" && depth === 0) {
      parts.push(token.slice(start, i));
      start = i + 1;
    }
  }
  return { variants: parts, base: token.slice(start) };
}

export function sizeChangingStateClasses(className: string): string[] {
  return className.split(/\s+/).filter((token) => {
    const { variants, base } = splitVariants(token);
    return variants.some((v) => stateVariant.test(v)) && sizeUtility.test(base);
  });
}
