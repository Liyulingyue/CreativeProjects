#[cfg(windows)]
pub fn relaunch_as_admin() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let verb: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    let code = result as isize;
    if code <= 32 {
        return Err(format!("提权启动失败（ShellExecute 返回 {code}）"));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn relaunch_as_admin() -> Result<(), String> {
    Err("仅支持 Windows".to_string())
}
