//! @argfile の文字コード。

use crate::error::CoreError;

/// argfile の中身を、Java が読む文字コードのバイト列にする。
/// Windows の Java は argfile を ANSI コードページで読むため、`GetACP()` のコードページで書く。
/// それ以外の OS では UTF-8 で書く。
#[cfg(windows)]
pub fn encode(contents: &str) -> Result<Vec<u8>, CoreError> {
    // SAFETY: 引数のない読み取りだけの API
    let code_page = unsafe { windows_sys::Win32::Globalization::GetACP() };
    encode_in(contents, code_page)
}

#[cfg(not(windows))]
pub fn encode(contents: &str) -> Result<Vec<u8>, CoreError> {
    Ok(contents.as_bytes().to_vec())
}

/// 指定したコードページで書く。表せない文字（近い文字への置き換えを含む）があれば
/// `PathNotEncodable` とする。
#[cfg(windows)]
fn encode_in(contents: &str, code_page: u32) -> Result<Vec<u8>, CoreError> {
    use std::ptr;
    use windows_sys::Win32::Globalization::{CP_UTF8, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte};

    // UTF-8 のコードページでは WC_NO_BEST_FIT_CHARS と lpUsedDefaultChar を使えない。
    // &str はすべての文字を表せるため、そのまま返す
    if code_page == CP_UTF8 {
        return Ok(contents.as_bytes().to_vec());
    }
    if contents.is_empty() {
        return Ok(Vec::new());
    }
    let wide: Vec<u16> = contents.encode_utf16().collect();
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::Globalization::GetACP;

    #[test]
    fn encodes_in_the_ansi_code_page() {
        // SAFETY: 引数のない読み取りだけの API
        let acp = unsafe { GetACP() };
        match acp {
            932 => assert_eq!(
                encode("C:\\ユーザー\\a.jar").unwrap(),
                b"C:\\\x83\x86\x81[\x83U\x81[\\a.jar".to_vec()
            ),
            1252 => assert_eq!(encode("C:\\é\\a.jar").unwrap(), b"C:\\\xe9\\a.jar".to_vec()),
            _ => {}
        }
        assert_eq!(
            encode("C:\\a b\\x.jar\n").unwrap(),
            b"C:\\a b\\x.jar\n".to_vec()
        );
        assert_eq!(encode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn unrepresentable_characters_are_an_error() {
        assert_eq!(
            encode("C:\\😀\\x.jar").unwrap_err().code(),
            "launch.path_not_encodable"
        );
    }

    #[test]
    fn explicit_code_pages() {
        assert_eq!(
            encode_in("C:\\é\\a.jar", 1252).unwrap(),
            b"C:\\\xe9\\a.jar".to_vec()
        );
        // 1252 には「あ」がない。近い文字への置き換え（best fit）もしない
        assert_eq!(
            encode_in("C:\\あ", 1252).unwrap_err().code(),
            "launch.path_not_encodable"
        );
        assert_eq!(
            encode_in("C:\\ā", 1252).unwrap_err().code(),
            "launch.path_not_encodable"
        );
    }

    #[test]
    fn utf8_code_page_returns_utf8() {
        let s = "C:\\ユーザー\\😀\\a.jar";
        assert_eq!(encode_in(s, 65001).unwrap(), s.as_bytes().to_vec());
    }
}
