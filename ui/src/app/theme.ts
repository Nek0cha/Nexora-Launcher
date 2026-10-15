import type { Theme } from "../bindings/Theme";

const QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(pref: Theme, prefersDark: boolean): "dark" | "light" {
  if (pref === "system") return prefersDark ? "dark" : "light";
  return pref;
}

/** テーマを <html data-theme> に反映し、最初の描画用の控えも更新する。解除関数を返す。 */
export function applyTheme(pref: Theme): () => void {
  const media = window.matchMedia(QUERY);
  const set = () => {
    document.documentElement.dataset.theme = resolveTheme(pref, media.matches);
  };
  set();
  try {
    localStorage.setItem("nexora.theme", pref);
  } catch {
    // 保存できない環境では控えを持たない
  }
  if (pref !== "system") return () => {};
  media.addEventListener("change", set);
  return () => media.removeEventListener("change", set);
}
