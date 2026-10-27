/**
 * 配置の安定の規則を確かめるためのテスト用の道具。
 * 状態（選択、押下、無効など）の接頭辞が付いたクラスのうち、寸法を変えるもの
 * （枠線の太さ、文字の太さや大きさ、余白、幅、高さ）を取り出す。状態は色、背景、影、輪郭だけで示す。
 */
const statePrefix =
  /^(?:[\w@-]+:)*(?:selected|pressed|hover|hovered|focus|focused|focus-visible|disabled|checked|dragging|data-[\w-]+|aria-[\w-]+|group-[\w-]+|peer-[\w-]+):/;

const sizeUtility =
  /^-?(?:border(?:-[xytrblse])?(?:-\d+|-\[[^\]]+\])?|ring-\d+|font-(?:thin|extralight|light|normal|medium|semibold|bold|extrabold|black)|p[xytrblse]?-.+|m[xytrblse]?-.+|w-.+|h-.+|size-.+|min-[wh]-.+|max-[wh]-.+|gap(?:-[xy])?-.+|text-(?:xs|sm|base|lg|\d?xl)|tracking-.+|leading-.+)$/;

export function sizeChangingStateClasses(className: string): string[] {
  return className
    .split(/\s+/)
    .filter((token) => statePrefix.test(token) && sizeUtility.test(token.slice(token.lastIndexOf(":") + 1)));
}
