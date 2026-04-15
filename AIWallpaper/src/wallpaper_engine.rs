use windows_sys::Win32::Foundation::{HWND, LPARAM, BOOL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowA, FindWindowExA, SendMessageTimeoutA,
    SMTO_NORMAL,
};
use std::ptr;

/// 完全复刻 Lively Wallpaper (WinDesktopCore.cs) 的 WorkerW 定位逻辑
pub unsafe fn get_wallpaper_workerw() -> HWND {
    // 1. 找到 Progman 窗口
    let progman = FindWindowA(b"Progman\0".as_ptr(), ptr::null());
    if progman == 0 {
        return 0;
    }

    // 2. 发送 0x052C 消息触发 Windows 生成 WorkerW
    // Lively 参数: WPARAM=0xD (13), LPARAM=0x1
    let mut result: usize = 0;
    SendMessageTimeoutA(
        progman,
        0x052C,
        0xD,
        0x1,
        SMTO_NORMAL,
        1000,
        &mut result as *mut _ as *mut _,
    );

    let mut workerw: HWND = 0;

    // 3. EnumWindows 寻找包含 SHELLDLL_DefView 的窗口
    // 一旦找到，它的 NEXT SIBLING (下一个兄弟) 就是目标 WorkerW
    unsafe extern "system" fn enum_window(tophandle: HWND, lparam: LPARAM) -> BOOL {
        let workerw_out = lparam as *mut HWND;
        
        // 检查当前窗口是否有 SHELLDLL_DefView 子窗口
        let p = FindWindowExA(tophandle, 0, b"SHELLDLL_DefView\0".as_ptr(), ptr::null());
        
        if p != 0 {
            // 关键：复刻 Lively 逻辑 -> FindWindowEx(0, tophandle, "WorkerW", 0)
            // 这会找到紧跟在包含 SHELLDLL_DefView 窗口之后的类名为 WorkerW 的顶层窗口
            let next_workerw = FindWindowExA(0, tophandle, b"WorkerW\0".as_ptr(), ptr::null());
            if next_workerw != 0 {
                *workerw_out = next_workerw;
                return 0; // 停止遍历
            }
        }
        1 // 继续
    }

    EnumWindows(Some(enum_window), &mut workerw as *mut _ as LPARAM);

    // 4. Win11 Raised Desktop 特殊逻辑 (Lively 源码 L200)
    // 如果上面没找到，尝试在 Progman 下直接找 WorkerW
    if workerw == 0 {
        workerw = FindWindowExA(progman, 0, b"WorkerW\0".as_ptr(), ptr::null());
    }

    workerw
}
