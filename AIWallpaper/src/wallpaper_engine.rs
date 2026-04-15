use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowA, FindWindowExA, SendMessageTimeoutA, SMTO_NORMAL,
};

/// 注入壁纸层的核心逻辑
/// 1. 找到 Progman
/// 2. 发送 0x052C 信号触发 WorkerW 生成
/// 3. 找到那个被创建在图标下方的 WorkerW
pub unsafe fn get_wallpaper_workerw() -> HWND {
    // 1. 寻找 Progman 句柄
    let progman = FindWindowA("Progman\0".as_ptr(), null_mut());

    // 2. 发送 0x052C 消息，请求 Windows 创建一个 WorkerW 窗口
    SendMessageTimeoutA(
        progman,
        0x052C,
        0,
        0,
        SMTO_NORMAL,
        1000,
        null_mut(),
    );

    let mut workerw = 0 as HWND;

    // 3. 遍历所有顶级窗口，寻找类名为 "WorkerW" 且拥有 SHELLDLL_DefView 的兄弟窗口
    unsafe extern "system" fn enum_window(window: HWND, lparam: LPARAM) -> i32 {
        let p_workerw = lparam as *mut HWND;
        let shell_view = FindWindowExA(window, 0, "SHELLDLL_DefView\0".as_ptr(), null_mut());

        if shell_view != 0 {
            // 我们找到了包含 ShellView 的 WorkerW，真正的壁纸展示窗体是它的下一个兄弟
            *p_workerw = FindWindowExA(0, window, "WorkerW\0".as_ptr(), null_mut());
        }
        TRUE
    }

    EnumWindows(Some(enum_window), &mut workerw as *mut HWND as LPARAM);

    workerw
}
