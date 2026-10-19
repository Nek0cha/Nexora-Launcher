//! ゲームに「閉じて」と頼む。終了は待たない。

/// pid のプロセスに、閉じるよう頼む。ウィンドウが無い、シグナルが送れないときは何もしない。
///
/// Windows では、そのプロセスが持つ、表示中で所有者の無いトップレベルのウィンドウに
/// `WM_SYSCOMMAND`（`SC_CLOSE`）を送る。これはタイトルバーの X ボタンや Alt+F4 が送るものと同じ。
/// `WM_CLOSE` にしないのは、LWJGL 2（Minecraft 1.8.9、1.12.2）が `SC_CLOSE` しか閉じる要求として扱わず、
/// `WM_CLOSE` ではウィンドウが壊れるだけで JVM が動き続けるため。
/// NVIDIA や IME などの非表示の補助ウィンドウは、壊さないよう飛ばす。
/// ほかの OS では SIGTERM を送る。
/// 返すのは頼みを送れた相手の数（Windows では `SC_CLOSE` を送れたウィンドウの数、ほかの OS では 0 か 1）。
pub fn request_close(pid: u32) -> usize {
    let signalled = imp::request_close(pid);
    tracing::info!(pid, signalled, "requested the game process to close");
    if signalled == 0 {
        tracing::warn!(pid, "no window or process accepted the close request");
    }
    signalled
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
        SC_CLOSE, WM_SYSCOMMAND,
    };
    use windows_sys::core::BOOL;

    struct Query {
        pid: u32,
        signalled: usize,
    }

    pub fn request_close(pid: u32) -> usize {
        let mut query = Query { pid, signalled: 0 };
        // SAFETY: コールバックは EnumWindows の呼び出しの間だけ呼ばれ、
        // lparam はその間生きている query のローカル変数を指す
        unsafe {
            EnumWindows(Some(close_if_owned), &mut query as *mut Query as LPARAM);
        }
        query.signalled
    }

    unsafe extern "system" fn close_if_owned(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam は request_close が渡した、生きている Query を指し、列挙の間は他から触られない
        let query = unsafe { &mut *(lparam as *mut Query) };
        let mut owner = 0u32;
        // SAFETY: hwnd は EnumWindows が渡したウィンドウで、owner は書き込める u32
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        // SAFETY: hwnd は EnumWindows が渡したウィンドウ。無くなっていても 0 が返るだけ
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        // SAFETY: 同上。所有者がいなければ（トップレベルの独立したウィンドウなら）null が返る
        let unowned = unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
        if owner == query.pid && visible && unowned {
            // SAFETY: 送るだけで待たない。ウィンドウがすでに無ければ失敗（0）が返るだけで、数えない
            if unsafe { PostMessageW(hwnd, WM_SYSCOMMAND, SC_CLOSE as usize, 0) } != 0 {
                query.signalled += 1;
            }
        }
        // 同じプロセスのほかのウィンドウにも送るため、列挙を続ける
        1
    }
}

#[cfg(not(windows))]
mod imp {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

    pub fn request_close(pid: u32) -> usize {
        let mut sys = System::new();
        let pid = Pid::from_u32(pid);
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        match sys.process(pid) {
            Some(process) if process.kill_with(Signal::Term) == Some(true) => 1,
            _ => 0,
        }
    }
}
