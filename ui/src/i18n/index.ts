import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enCommon from "./en/common.json";
import enErrors from "./en/errors.json";
import enSettings from "./en/settings.json";
import jaCommon from "./ja/common.json";
import jaErrors from "./ja/errors.json";
import jaSettings from "./ja/settings.json";

export const resources = {
  ja: { common: jaCommon, errors: jaErrors, settings: jaSettings },
  en: { common: enCommon, errors: enErrors, settings: enSettings },
} as const;

export type Lang = keyof typeof resources;

export function detectLanguage(navigatorLanguage: string): Lang {
  return navigatorLanguage.toLowerCase().startsWith("ja") ? "ja" : "en";
}

void i18n.use(initReactI18next).init({
  resources,
  lng: detectLanguage(typeof navigator === "undefined" ? "en" : navigator.language),
  fallbackLng: "en",
  ns: ["common", "errors", "settings"],
  defaultNS: "common",
  interpolation: { escapeValue: false },
});

export async function setLanguage(lang: Lang): Promise<void> {
  await i18n.changeLanguage(lang);
  document.documentElement.lang = lang;
}

export default i18n;
