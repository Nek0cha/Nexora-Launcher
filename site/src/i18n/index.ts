import { en, type Dict } from "./en";
import { ja } from "./ja";

export type Lang = "en" | "ja";

export function dict(lang: Lang): Dict {
  return lang === "ja" ? ja : en;
}
