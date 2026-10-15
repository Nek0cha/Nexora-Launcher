//! ウィンドウの素材（Mica など）が実際に表示される見込みがあるかを判定する。

#[cfg(windows)]
pub fn material_available() -> bool {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

    let build_ok = LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|key| key.get_string("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(|build| build >= 22000);
    // 値がない場合は、Windows の既定（透明効果オン）とみなす
    let transparency_on = CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("EnableTransparency"))
        .map(|v| v == 1)
        .unwrap_or(true);
    build_ok && transparency_on
}

#[cfg(target_os = "macos")]
pub fn material_available() -> bool {
    true
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn material_available() -> bool {
    false
}
