//! 共通設定の登録表。同期してよい options.txt のキーだけをここに並べる。
//! 一覧に無いキー（解像度、フルスクリーン、lastServer、リソースパック等）は読み書きしない。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 共通設定の項目をまとめる種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
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
    /// この項目を扱う最小の DataVersion。`0` は制限なし。
    pub min_data_version: i64,
}

impl Field {
    /// この DataVersion のゲームで、項目を扱ってよいか。
    pub fn applies_to(&self, data_version: i64) -> bool {
        data_version >= self.min_data_version
    }
}

const fn f(key: &'static str, category: Category, kind: ValueKind) -> Field {
    f_since(key, category, kind, 0)
}

/// `min_data_version` 以上のゲームでだけ扱う項目。
#[allow(dead_code)]
pub(crate) const fn f_since(
    key: &'static str,
    category: Category,
    kind: ValueKind,
    min_data_version: i64,
) -> Field {
    Field {
        id: key,
        key,
        category,
        kind,
        min_data_version,
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
    f("soundCategory_ui", C::Sound, K::Float),
    f("directionalAudio", C::Sound, K::Bool),
    f("musicFrequency", C::Sound, K::Text),
    f("mouseSensitivity", C::Mouse, K::Float),
    f("invertYMouse", C::Mouse, K::Bool),
    f("discrete_mouse_scroll", C::Mouse, K::Bool),
    f("invertXMouse", C::Mouse, K::Bool),
    f("mouseWheelSensitivity", C::Mouse, K::Float),
    f("renderDistance", C::Graphics, K::Int),
    f("fov", C::Graphics, K::Float),
    f("gamma", C::Graphics, K::Float),
    f("maxFps", C::Graphics, K::Int),
    f("enableVsync", C::Graphics, K::Bool),
    f("guiScale", C::Graphics, K::Int),
    f("entityShadows", C::Graphics, K::Bool),
    f("bobView", C::Graphics, K::Bool),
    f("damageTiltStrength", C::Graphics, K::Float),
    f("glintSpeed", C::Graphics, K::Float),
    f("glintStrength", C::Graphics, K::Float),
    f("darknessEffectScale", C::Graphics, K::Float),
    f("screenEffectScale", C::Graphics, K::Float),
    f("fovEffectScale", C::Graphics, K::Float),
    f("hideLightningFlashes", C::Graphics, K::Bool),
    f("menuBackgroundBlurriness", C::Graphics, K::Int),
    f("particles", C::Graphics, K::Int),
    f("chatOpacity", C::Chat, K::Float),
    f("chatScale", C::Chat, K::Float),
    f("chatWidth", C::Chat, K::Float),
    f("chatHeightFocused", C::Chat, K::Float),
    f("chatHeightUnfocused", C::Chat, K::Float),
    f("chatLinks", C::Chat, K::Bool),
    f("chatColors", C::Chat, K::Bool),
    f("chatLinksPrompt", C::Chat, K::Bool),
    f("chatLineSpacing", C::Chat, K::Float),
    f("textBackgroundOpacity", C::Chat, K::Float),
    f("autoSuggestions", C::Chat, K::Bool),
    f("narrator", C::Accessibility, K::Int),
    f("highContrast", C::Accessibility, K::Bool),
    f("autoJump", C::Accessibility, K::Bool),
    f("toggleCrouch", C::Accessibility, K::Bool),
    f("toggleSprint", C::Accessibility, K::Bool),
    f("toggleAttack", C::Accessibility, K::Bool),
    f("toggleUse", C::Accessibility, K::Bool),
    f("sprintWindow", C::Accessibility, K::Int),
    f("attackIndicator", C::Accessibility, K::Int),
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
    fn applies_to_respects_the_floor() {
        let x = f_since("x", C::Graphics, K::Int, 819);
        assert!(!x.applies_to(818));
        assert!(x.applies_to(819));
        assert!(x.applies_to(4325));
        let free = f("y", C::Graphics, K::Int);
        assert!(free.applies_to(0));
    }

    #[test]
    fn existing_fields_have_no_floor() {
        for x in fields() {
            assert_eq!(x.min_data_version, 0, "{}", x.id);
        }
        assert_eq!(
            field_for_key("key_some_mod.action")
                .unwrap()
                .min_data_version,
            0
        );
    }

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
            // 版によって値の引用符が違う（1.19 以降は "right"、1.13〜1.18 は right）ため同期しない
            "mainHand",
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

    const REAL: &str = include_str!("../../tests/fixtures/options/real_modern.txt");

    #[test]
    fn registry_keys_match_the_real_sample() {
        // 古い版にだけ存在すると分かっているキー（現状なし）
        let legacy_only: &[&str] = &[];
        let sample = crate::sync::options::OptionsFile::parse(REAL);
        for x in fields() {
            if x.key.starts_with("key_") || legacy_only.contains(&x.key) {
                continue;
            }
            assert!(sample.get(x.key).is_some(), "サンプルに無いキー: {}", x.key);
        }
    }

    #[test]
    fn real_sample_round_trips() {
        let sample = crate::sync::options::OptionsFile::parse(REAL);
        assert_eq!(sample.render(), REAL);
    }

    #[test]
    fn real_sample_key_bindings_are_keyboard_codec() {
        let sample = crate::sync::options::OptionsFile::parse(REAL);
        let keys: Vec<_> = sample
            .entries()
            .into_iter()
            .filter(|(k, _)| k.starts_with("key_"))
            .collect();
        assert!(!keys.is_empty());
        for (k, v) in keys {
            assert_eq!(key_codec(v), KeyCodec::Keyboard, "{k}={v}");
        }
    }

    #[test]
    fn excluded_keys_stay_unregistered_in_the_real_sample() {
        let sample = crate::sync::options::OptionsFile::parse(REAL);
        for key in [
            "overrideWidth",
            "overrideHeight",
            "fullscreen",
            "soundDevice",
            "resourcePacks",
            "lastServer",
            "tutorialStep",
            "telemetryOptInExtra",
            "onboardAccessibility",
            "skipMultiplayerWarning",
            "joinedFirstServer",
            "graphicsPreset",
            "preferredGraphicsBackend",
            "textureFiltering",
            "improvedTransparency",
            "lang",
            "modelPart_cape",
            "realmsNotifications",
            "sharePresence",
            // 版によって値の引用符が違う（1.19 以降は "right"、1.13〜1.18 は right）ため同期しない
            "mainHand",
        ] {
            assert!(sample.get(key).is_some(), "サンプルに無い: {key}");
            assert!(field_for_key(key).is_none(), "{key}");
        }
    }

    #[test]
    fn newly_registered_real_keys_resolve() {
        for (key, cat) in [
            ("discrete_mouse_scroll", Category::Mouse),
            ("bobView", Category::Graphics),
            ("invertXMouse", Category::Mouse),
            ("soundCategory_ui", Category::Sound),
            ("textBackgroundOpacity", Category::Chat),
        ] {
            assert_eq!(field_for_key(key).unwrap().category, cat, "{key}");
        }
        assert!(field_for_key("rawMouseInput").is_none());
    }
}
