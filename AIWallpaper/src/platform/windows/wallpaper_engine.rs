use std::ptr;

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowA, FindWindowExA, SendMessageTimeoutA, SMTO_NORMAL,
};

/// 完全复刻 Lively Wallpaper (WinDesktopCore.cs) 的 WorkerW 定位逻辑
pub unsafe fn get_wallpaper_workerw() -> HWND {
    let progman = FindWindowA(b"Progman\0".as_ptr(), ptr::null());
    if progman == 0 {
        return 0;
    }

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

    unsafe extern "system" fn enum_window(tophandle: HWND, lparam: LPARAM) -> BOOL {
        let workerw_out = lparam as *mut HWND;
        let p = FindWindowExA(tophandle, 0, b"SHELLDLL_DefView\0".as_ptr(), ptr::null());

        if p != 0 {
            let next_workerw = FindWindowExA(0, tophandle, b"WorkerW\0".as_ptr(), ptr::null());
            if next_workerw != 0 {
                *workerw_out = next_workerw;
                return 0;
            }
        }

        1
    }

    EnumWindows(Some(enum_window), &mut workerw as *mut _ as LPARAM);

    if workerw == 0 {
        workerw = FindWindowExA(progman, 0, b"WorkerW\0".as_ptr(), ptr::null());
    }

    workerw
}
