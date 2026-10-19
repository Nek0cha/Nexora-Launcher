//! ゲームに「閉じて」と頼む。終了は待たない。

/// pid のプロセスに、閉じるよう頼む。ウィンドウが無い、シグナルが送れないときは何もしない。
///
/// Windows では、そのプロセスが持つトップレベルのウィンドウすべてに `WM_CLOSE` を送る。
/// ほかの OS では SIGTERM を送る。
pub fn request_close(pid: u32) {
    imp::request_close(pid);
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
    };
    use windows_sys::core::BOOL;

    pub fn request_close(pid: u32) {
        // SAFETY: コールバックは EnumWindows の呼び出しの間だけ呼ばれ、
        // lparam はその間生きている pid のローカル変数を指す
        unsafe {
            EnumWindows(Some(close_if_owned), &pid as *const u32 as LPARAM);
        }
    }

    unsafe extern "system" fn close_if_owned(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam は request_close が渡した、生きている u32 を指す
        let target = unsafe { *(lparam as *const u32) };
        let mut owner = 0u32;
        // SAFETY: hwnd は EnumWindows が渡したウィンドウで、owner は書き込める u32
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        if owner == target {
            // SAFETY: 送るだけで待たない。ウィンドウがすでに無ければ失敗が返るだけで、無視してよい
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
        }
        // 同じプロセスのほかのウィンドウにも送るため、列挙を続ける
        1
    }
}

#[cfg(not(windows))]
mod imp {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

    pub fn request_close(pid: u32) {
        let mut sys = System::new();
        let pid = Pid::from_u32(pid);
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        if let Some(process) = sys.process(pid) {
            let _ = process.kill_with(Signal::Term);
        }
    }
}
