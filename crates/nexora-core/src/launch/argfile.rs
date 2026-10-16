//! Java の @argfile の書き出し（書式と文字コード）。

use crate::error::CoreError;

/// 引数を argfile のバイト列にする。各引数を Java が読む文字コードにしてから、
/// 二重引用符で囲み、`\` と `"` をエスケープする。
///
/// Windows の Java は argfile を ANSI コードページで読むため、`GetACP()` のコードページで書く。
/// それ以外の OS では UTF-8 で書く。
/// Java は argfile をバイト単位で読むため、エスケープは文字コードを変えた後のバイト列に行う
/// （CP932 などでは、「表」や「ソ」の 2 バイト目が 0x5C（`\`）になるため）。
#[cfg(windows)]
pub fn encode(args: &[String]) -> Result<Vec<u8>, CoreError> {
    // SAFETY: 引数のない読み取りだけの API
    let code_page = unsafe { windows_sys::Win32::Globalization::GetACP() };
    encode_in(args, code_page)
}

#[cfg(not(windows))]
pub fn encode(args: &[String]) -> Result<Vec<u8>, CoreError> {
    build(args, |s| Ok(s.as_bytes().to_vec()))
}

/// 指定したコードページで argfile を書く。
#[cfg(windows)]
fn encode_in(args: &[String], code_page: u32) -> Result<Vec<u8>, CoreError> {
    build(args, |s| to_code_page(s, code_page))
}

fn build(
    args: &[String],
    convert: impl Fn(&str) -> Result<Vec<u8>, CoreError>,
) -> Result<Vec<u8>, CoreError> {
    let mut out = Vec::new();
    for arg in args {
        out.push(b'"');
        for byte in convert(arg)? {
            if byte == b'\\' || byte == b'"' {
                out.push(b'\\');
            }
            out.push(byte);
        }
        out.extend_from_slice(b"\"\n");
    }
    Ok(out)
}

/// 文字列をコードページのバイト列にする。表せない文字（近い文字への置き換えを含む）があれば
/// `PathNotEncodable` とする。
#[cfg(windows)]
fn to_code_page(text: &str, code_page: u32) -> Result<Vec<u8>, CoreError> {
    use std::ptr;
    use windows_sys::Win32::Globalization::{CP_UTF8, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte};

    // UTF-8 のコードページでは WC_NO_BEST_FIT_CHARS と lpUsedDefaultChar を使えない。
    // &str はすべての文字を表せるため、そのまま返す
    if code_page == CP_UTF8 {
        return Ok(text.as_bytes().to_vec());
    }
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let wide_len = i32::try_from(wide.len()).map_err(|_| CoreError::Unexpected {
        message: "argfile is too large to convert".into(),
    })?;
    let convert = |out: *mut u8, out_len: i32, used_default: &mut i32| {
        // SAFETY: wide は wide_len 個の u16 を持つ。out は null（長さの問い合わせ）か、
        // out_len バイトを書ける領域を指す
        unsafe {
            WideCharToMultiByte(
                code_page,
                WC_NO_BEST_FIT_CHARS,
                wide.as_ptr(),
                wide_len,
                out,
                out_len,
                ptr::null(),
                used_default,
            )
        }
    };
    let mut used_default = 0;
    let needed = convert(ptr::null_mut(), 0, &mut used_default);
    if needed <= 0 {
        return Err(conversion_failed(code_page));
    }
    if used_default != 0 {
        return Err(CoreError::PathNotEncodable);
    }
    let mut out = vec![0u8; needed as usize];
    let written = convert(out.as_mut_ptr(), needed, &mut used_default);
    if written <= 0 {
        return Err(conversion_failed(code_page));
    }
    if used_default != 0 {
        return Err(CoreError::PathNotEncodable);
    }
    out.truncate(written as usize);
    Ok(out)
}

#[cfg(windows)]
fn conversion_failed(code_page: u32) -> CoreError {
    CoreError::Unexpected {
        message: format!(
            "failed to convert the argfile to code page {code_page}: {}",
            std::io::Error::last_os_error()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn quotes_and_escapes() {
        let bytes = encode(&strings(&[r#"C:\a b\c"#, r#"say "hi""#])).unwrap();
        assert_eq!(bytes, b"\"C:\\\\a b\\\\c\"\n\"say \\\"hi\\\"\"\n".to_vec());
        assert_eq!(encode(&strings(&[""])).unwrap(), b"\"\"\n".to_vec());
        assert_eq!(encode(&[]).unwrap(), Vec::<u8>::new());
    }

    #[cfg(windows)]
    #[test]
    fn encodes_in_the_ansi_code_page() {
        use windows_sys::Win32::Globalization::GetACP;
        // SAFETY: 引数のない読み取りだけの API
        let acp = unsafe { GetACP() };
        match acp {
            932 => assert_eq!(
                encode(&strings(&["C:\\ユーザー\\a.jar"])).unwrap(),
                b"\"C:\\\\\x83\x86\x81[\x83U\x81[\\\\a.jar\"\n".to_vec()
            ),
            1252 => assert_eq!(
                encode(&strings(&["C:\\é\\a.jar"])).unwrap(),
                b"\"C:\\\\\xe9\\\\a.jar\"\n".to_vec()
            ),
            _ => {}
        }
    }

    #[cfg(windows)]
    #[test]
    fn unrepresentable_characters_are_an_error() {
        assert_eq!(
            encode(&strings(&["-cp", "C:\\😀\\x.jar"]))
                .unwrap_err()
                .code(),
            "launch.path_not_encodable"
        );
    }

    #[cfg(windows)]
    #[test]
    fn explicit_code_pages() {
        assert_eq!(
            encode_in(&strings(&["C:\\é\\a.jar"]), 1252).unwrap(),
            b"\"C:\\\\\xe9\\\\a.jar\"\n".to_vec()
        );
        // 1252 には「あ」がない。近い文字への置き換え（best fit）もしない
        assert_eq!(
            encode_in(&strings(&["C:\\あ"]), 1252).unwrap_err().code(),
            "launch.path_not_encodable"
        );
        assert_eq!(
            encode_in(&strings(&["C:\\ā"]), 1252).unwrap_err().code(),
            "launch.path_not_encodable"
        );
    }

    #[cfg(windows)]
    #[test]
    fn dbcs_trail_bytes_are_escaped() {
        // CP932 の「表」は 0x95 0x5C。2 バイト目の 0x5C も Java はエスケープとして読む
        assert_eq!(
            encode_in(&strings(&["C:\\表\\a.jar"]), 932).unwrap(),
            b"\"C:\\\\\x95\x5C\x5C\\\\a.jar\"\n".to_vec()
        );
        // 「ソ」は 0x83 0x5C
        assert_eq!(
            encode_in(&strings(&["ソ\""]), 932).unwrap(),
            b"\"\x83\x5C\x5C\\\"\"\n".to_vec()
        );
    }

    #[cfg(windows)]
    #[test]
    fn utf8_code_page_returns_utf8() {
        // UTF-8 では 0x5C が多バイト文字の中に現れないため、Unicode の文字列でエスケープした結果と同じになる
        let s = "C:\\ユーザー\\😀\\表ソ\\\"a\".jar";
        let expected = format!("\"{}\"\n", s.replace('\\', "\\\\").replace('"', "\\\""));
        assert_eq!(
            encode_in(&strings(&[s]), 65001).unwrap(),
            expected.into_bytes()
        );
    }
}
