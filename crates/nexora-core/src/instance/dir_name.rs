//! インスタンスの名前から、OS で使えるフォルダ名を決める。

/// フォルダ名の最大の文字数。パスを短く保つため（Java 8 のコマンドラインの上限と MAX_PATH）。
pub const MAX_CHARS: usize = 64;

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_forbidden(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
}

/// OS で使えない文字を `_` にし、末尾の空白とピリオドを除き、予約名を避ける。
/// どの OS でも同じ結果にする（OS をまたいでデータを移しても使えるように）。
pub fn sanitize(name: &str) -> String {
    let replaced: String = name
        .trim()
        .chars()
        .map(|c| if is_forbidden(c) { '_' } else { c })
        .take(MAX_CHARS)
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        return "instance".to_string();
    }
    let (stem, rest) = match trimmed.find('.') {
        Some(i) => trimmed.split_at(i),
        None => (trimmed.as_str(), ""),
    };
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        let escaped = format!("{stem}_{rest}");
        return escaped.chars().take(MAX_CHARS).collect();
    }
    trimmed
}

/// base が使われていれば、末尾に ` (2)`、` (3)` … を付けて、使われていない名前を返す。
pub fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|candidate| !taken(candidate))
        .expect("an unused name always exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ordinary_and_non_ascii_names() {
        assert_eq!(sanitize("My World"), "My World");
        assert_eq!(sanitize("日本語のワールド"), "日本語のワールド");
        assert_eq!(sanitize("1.21.5"), "1.21.5");
    }

    #[test]
    fn replaces_forbidden_characters() {
        assert_eq!(sanitize(r#"a<b>c:d"e/f\g|h?i*j"#), "a_b_c_d_e_f_g_h_i_j");
        assert_eq!(sanitize("tab\there"), "tab_here");
    }

    #[test]
    fn trims_trailing_dots_and_spaces() {
        assert_eq!(sanitize("  name. . "), "name");
        assert_eq!(sanitize("..."), "instance");
        assert_eq!(sanitize("   "), "instance");
        assert_eq!(sanitize(""), "instance");
    }

    #[test]
    fn escapes_windows_reserved_names() {
        assert_eq!(sanitize("CON"), "CON_");
        assert_eq!(sanitize("nul"), "nul_");
        assert_eq!(sanitize("com1.txt"), "com1_.txt");
        assert_eq!(sanitize("console"), "console");
    }

    #[test]
    fn limits_length_in_characters() {
        let long = "あ".repeat(100);
        assert_eq!(sanitize(&long).chars().count(), MAX_CHARS);
    }

    #[test]
    fn unique_appends_counter_case_insensitively() {
        let taken = ["a", "A (2)"];
        let is_taken = |n: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(n));
        assert_eq!(unique("a", is_taken), "a (3)");
        assert_eq!(unique("b", is_taken), "b");
    }
}
