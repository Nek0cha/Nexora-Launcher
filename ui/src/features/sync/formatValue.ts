/** 値の整形で使う文言のキー */
type ValueKey =
  | "sync.value.on"
  | "sync.value.off"
  | "sync.value.auto"
  | "sync.value.unlimited"
  | "sync.value.left"
  | "sync.value.right"
  | "sync.keys.unbound"
  | "sync.keys.space"
  | "sync.keys.mouseLeft"
  | "sync.keys.mouseRight"
  | "sync.keys.mouseMiddle";
type KeyWithArg = "sync.keys.left" | "sync.keys.right" | "sync.keys.keypad";

/** 文言を引く関数。i18next の t をそのまま渡せる */
export interface Translate {
  (key: ValueKey): string;
  (key: KeyWithArg, options: { key: string }): string;
  (key: "sync.keys.mouseButton", options: { n: string }): string;
}

// 0 から 1 の値を、ゲームの画面と同じく百分率で出す項目
const PERCENT_FIELDS = new Set([
  "gamma",
  "chatOpacity",
  "textBackgroundOpacity",
  "chatLineSpacing",
  "chatScale",
  "darknessEffectScale",
  "screenEffectScale",
  "fovEffectScale",
  "damageTiltStrength",
  "glintSpeed",
  "glintStrength",
]);

// 最大フレームレートのこの値は「無制限」
const UNLIMITED_FPS = 260;

const NUMBER = /^-?\d+(\.\d+)?$/;

/** 小数第 2 位までに丸め、末尾の 0 を落とす */
function round(n: number): string {
  return String(Number(n.toFixed(2)));
}

/**
 * 設定の値を画面に出す形にする。真偽はオン・オフ、数値は丸め（音量などは百分率、視野角は度）、
 * キー割り当てはキー名。知らない書式の文字列はそのまま出す。
 */
export function formatValue(fieldId: string, raw: string, t: Translate): string {
  // 古いファイルでは文字列が引用符で囲まれている
  const value = raw.length >= 2 && raw.startsWith('"') && raw.endsWith('"') ? raw.slice(1, -1) : raw;
  if (fieldId.startsWith("key_")) return formatKey(value, t);
  if (value === "true") return t("sync.value.on");
  if (value === "false") return t("sync.value.off");
  if (fieldId === "mainHand" && (value === "left" || value === "right")) return t(`sync.value.${value}`);
  if (!NUMBER.test(value)) return value;

  const n = Number(value);
  if (fieldId === "fov") return `${Math.round(70 + n * 40)}°`;
  if (fieldId === "mouseSensitivity") return `${Math.round(n * 200)}%`;
  if (fieldId.startsWith("soundCategory_") || PERCENT_FIELDS.has(fieldId)) return `${Math.round(n * 100)}%`;
  if (fieldId === "guiScale" && n === 0) return t("sync.value.auto");
  if (fieldId === "maxFps" && n >= UNLIMITED_FPS) return t("sync.value.unlimited");
  return round(n);
}

// 名前が記号や略称になるキー
const NAMED_KEYS: Record<string, string> = {
  enter: "Enter",
  tab: "Tab",
  escape: "Esc",
  backspace: "Backspace",
  insert: "Insert",
  delete: "Delete",
  home: "Home",
  end: "End",
  menu: "Menu",
  pause: "Pause",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  "caps.lock": "Caps Lock",
  "num.lock": "Num Lock",
  "scroll.lock": "Scroll Lock",
  "print.screen": "Print Screen",
  "page.up": "Page Up",
  "page.down": "Page Down",
  "grave.accent": "`",
  minus: "-",
  equal: "=",
  "left.bracket": "[",
  "right.bracket": "]",
  backslash: "\\",
  semicolon: ";",
  apostrophe: "'",
  comma: ",",
  period: ".",
  slash: "/",
};

const MODIFIERS: Record<string, string> = { shift: "Shift", control: "Ctrl", alt: "Alt", win: "Win" };

const KEYPAD: Record<string, string> = {
  add: "+",
  subtract: "-",
  multiply: "*",
  divide: "/",
  decimal: ".",
  equal: "=",
  enter: "Enter",
};

const capitalize = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);

/** `key.keyboard.*` と `key.mouse.*` の値をキー名にする。それ以外の値はそのまま返す */
export function formatKey(value: string, t: Translate): string {
  if (value.startsWith("key.mouse.")) {
    const button = value.slice("key.mouse.".length);
    if (button === "left") return t("sync.keys.mouseLeft");
    if (button === "right") return t("sync.keys.mouseRight");
    if (button === "middle") return t("sync.keys.mouseMiddle");
    return t("sync.keys.mouseButton", { n: button });
  }
  if (!value.startsWith("key.keyboard.")) return value;

  const name = value.slice("key.keyboard.".length);
  if (name === "unknown") return t("sync.keys.unbound");
  if (name === "space") return t("sync.keys.space");
  const named = NAMED_KEYS[name];
  if (named) return named;
  if (/^[a-z0-9]$/.test(name)) return name.toUpperCase();
  if (/^f\d{1,2}$/.test(name)) return name.toUpperCase();

  const side = /^(left|right)\.(shift|control|alt|win)$/.exec(name);
  if (side) return t(side[1] === "left" ? "sync.keys.left" : "sync.keys.right", { key: MODIFIERS[side[2]!]! });

  if (name.startsWith("keypad.")) {
    const key = name.slice("keypad.".length);
    return t("sync.keys.keypad", { key: KEYPAD[key] ?? key.toUpperCase() });
  }
  return name.split(".").map(capitalize).join(" ");
}
