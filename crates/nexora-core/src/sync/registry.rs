//! 共通設定の登録表。同期してよい options.txt のキーだけをここに並べる。
//! 一覧に無いキー（解像度、フルスクリーン、lastServer、リソースパック等）は読み書きしない。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// 共通設定の項目をまとめる種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Keys,
    Mouse,
    Sound,
    Graphics,
    Chat,
    Accessibility,
}

/// 値の型。options.txt の値は文字列なので、画面での扱いを決めるためだけに持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Bool,
    Int,
    Float,
    Text,
}

/// 登録表の 1 項目。`id` は `key` と同じ文字列で、一意かつ安定している。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub id: &'static str,
    pub key: &'static str,
    pub category: Category,
    pub kind: ValueKind,
}

const fn f(key: &'static str, category: Category, kind: ValueKind) -> Field {
    Field {
        id: key,
        key,
        category,
        kind,
    }
}

const fn key_field(key: &'static str) -> Field {
    f(key, Category::Keys, ValueKind::Text)
}

use Category as C;
use ValueKind as K;

/// 登録済みの項目。`key_` の項目は標準のものだけを並べる
/// （Mod 追加のキー割り当ては `field_for_key` が同じ扱いで返す）。
static FIELDS: &[Field] = &[
    f("soundCategory_master", C::Sound, K::Float),
    f("soundCategory_music", C::Sound, K::Float),
    f("soundCategory_record", C::Sound, K::Float),
    f("soundCategory_weather", C::Sound, K::Float),
    f("soundCategory_block", C::Sound, K::Float),
    f("soundCategory_hostile", C::Sound, K::Float),
    f("soundCategory_neutral", C::Sound, K::Float),
    f("soundCategory_player", C::Sound, K::Float),
    f("soundCategory_ambient", C::Sound, K::Float),
    f("soundCategory_voice", C::Sound, K::Float),
    f("mouseSensitivity", C::Mouse, K::Float),
    f("invertYMouse", C::Mouse, K::Bool),
    f("discreteMouseScroll", C::Mouse, K::Bool),
    f("mouseWheelSensitivity", C::Mouse, K::Float),
    f("rawMouseInput", C::Mouse, K::Bool),
    f("renderDistance", C::Graphics, K::Int),
    f("fov", C::Graphics, K::Float),
    f("gamma", C::Graphics, K::Float),
    f("maxFps", C::Graphics, K::Int),
    f("enableVsync", C::Graphics, K::Bool),
    f("guiScale", C::Graphics, K::Int),
    f("entityShadows", C::Graphics, K::Bool),
    f("viewBobbing", C::Graphics, K::Bool),
    f("particles", C::Graphics, K::Int),
    f("chatOpacity", C::Chat, K::Float),
    f("chatScale", C::Chat, K::Float),
    f("chatWidth", C::Chat, K::Float),
    f("chatHeightFocused", C::Chat, K::Float),
    f("chatHeightUnfocused", C::Chat, K::Float),
    f("chatLinks", C::Chat, K::Bool),
    f("chatColors", C::Chat, K::Bool),
    f("chatLinksPrompt", C::Chat, K::Bool),
    f("narrator", C::Accessibility, K::Int),
    f("highContrast", C::Accessibility, K::Bool),
    f("autoJump", C::Accessibility, K::Bool),
    f("toggleCrouch", C::Accessibility, K::Bool),
    f("toggleSprint", C::Accessibility, K::Bool),
    f("showSubtitles", C::Accessibility, K::Bool),
    key_field("key_key.attack"),
    key_field("key_key.use"),
    key_field("key_key.forward"),
    key_field("key_key.left"),
    key_field("key_key.back"),
    key_field("key_key.right"),
    key_field("key_key.jump"),
    key_field("key_key.sneak"),
    key_field("key_key.sprint"),
    key_field("key_key.drop"),
    key_field("key_key.inventory"),
    key_field("key_key.chat"),
    key_field("key_key.playerlist"),
    key_field("key_key.pickItem"),
    key_field("key_key.command"),
    key_field("key_key.socialInteractions"),
    key_field("key_key.screenshot"),
    key_field("key_key.togglePerspective"),
    key_field("key_key.smoothCamera"),
    key_field("key_key.fullscreen"),
    key_field("key_key.spectatorOutlines"),
    key_field("key_key.swapOffhand"),
    key_field("key_key.saveToolbarActivator"),
    key_field("key_key.loadToolbarActivator"),
    key_field("key_key.advancements"),
    key_field("key_key.hotbar.1"),
    key_field("key_key.hotbar.2"),
    key_field("key_key.hotbar.3"),
    key_field("key_key.hotbar.4"),
    key_field("key_key.hotbar.5"),
    key_field("key_key.hotbar.6"),
    key_field("key_key.hotbar.7"),
    key_field("key_key.hotbar.8"),
    key_field("key_key.hotbar.9"),
];

/// 登録済みの項目の一覧。
pub fn fields() -> &'static [Field] {
    FIELDS
}

/// Mod 由来など、一覧に無い `key_` のキーを `'static` にして使い回すための表。
fn interned_keys() -> &'static Mutex<HashMap<String, &'static str>> {
    static MAP: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// options.txt のキーに対応する項目を返す。登録外は `None`。
/// `key_` で始まるキーはすべて Keys の 1 項目として扱い、`id` はキーそのものにする。
pub fn field_for_key(key: &str) -> Option<Field> {
    if let Some(found) = FIELDS.iter().find(|x| x.key == key) {
        return Some(*found);
    }
    // `key_` 単独や、行の区切りを含む不正なキーは項目にしない
    if key.len() > "key_".len() && key.starts_with("key_") && !key.contains(['\n', '\r', ':']) {
        let mut map = interned_keys().lock().unwrap_or_else(|e| e.into_inner());
        let leaked = *map
            .entry(key.to_string())
            .or_insert_with(|| Box::leak(key.to_string().into_boxed_str()));
        return Some(key_field(leaked));
    }
    None
}

/// キー割り当ての値の書式。バージョンをまたぐ変換は行わず、判定だけする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCodec {
    /// 旧式（LWJGL 2）の整数コード
    Lwjgl2,
    /// `key.keyboard.w` や `key.mouse.left` の形
    Keyboard,
    Unknown,
}

/// 値の形から書式を判定する。
pub fn key_codec(value: &str) -> KeyCodec {
    let v = value.trim();
    if !v.is_empty() && v.parse::<i64>().is_ok() {
        KeyCodec::Lwjgl2
    } else if v.starts_with("key.keyboard.") || v.starts_with("key.mouse.") {
        KeyCodec::Keyboard
    } else {
        KeyCodec::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_has_unique_ids() {
        let mut seen = HashSet::new();
        for x in fields() {
            assert_eq!(x.id, x.key);
            assert!(seen.insert(x.id), "重複: {}", x.id);
        }
    }

    #[test]
    fn excluded_keys_are_not_registered() {
        for key in [
            "overrideWidth",
            "overrideHeight",
            "fullscreen",
            "fullscreenResolution",
            "lastServer",
            "soundDevice",
            "resourcePacks",
            "incompatibleResourcePacks",
            "tutorialStep",
            "telemetryOptInExtra",
            "onboardAccessibility",
            "skipMultiplayerWarning",
            "fancyGraphics",
            "graphicsMode",
            "key_",
        ] {
            assert!(field_for_key(key).is_none(), "{key}");
        }
    }

    #[test]
    fn key_family_and_known_fields_resolve() {
        let a = field_for_key("key_key.attack").unwrap();
        assert_eq!((a.id, a.category), ("key_key.attack", Category::Keys));
        let m = field_for_key("key_some_mod.action").unwrap();
        assert_eq!((m.id, m.category), ("key_some_mod.action", Category::Keys));
        assert_eq!(field_for_key("fov").unwrap().category, Category::Graphics);
    }

    #[test]
    fn key_codec_detects_formats() {
        assert_eq!(key_codec("30"), KeyCodec::Lwjgl2);
        assert_eq!(key_codec("-100"), KeyCodec::Lwjgl2);
        assert_eq!(key_codec("key.keyboard.w"), KeyCodec::Keyboard);
        assert_eq!(key_codec("key.mouse.left"), KeyCodec::Keyboard);
        assert_eq!(key_codec("scancode.4"), KeyCodec::Unknown);
        assert_eq!(key_codec(""), KeyCodec::Unknown);
    }
}
