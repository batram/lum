//! Foreground application detection for the per-app pause list.
//! Gets the process name of the currently focused window.

use std::ffi::c_void;

extern "system" {
    fn GetForegroundWindow() -> *mut c_void;
    fn GetWindowThreadProcessId(hwnd: *mut c_void, lpdwProcessId: *mut u32) -> u32;
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut c_void;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn QueryFullProcessImageNameW(
        hProcess: *mut c_void,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;
}

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

/// Get the executable name (e.g. "photoshop.exe") of the foreground window.
/// Returns None if it can't be determined.
pub fn get_foreground_process_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }

        let mut buf = [0u16; 260]; // MAX_PATH
        let mut size: u32 = buf.len() as u32;

        let result = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(process);

        if result == 0 {
            return None;
        }

        // Convert wide string to Rust string
        let full_path = String::from_utf16_lossy(&buf[..size as usize]);

        // Extract just the filename (e.g. "C:\Program Files\...\photoshop.exe" → "photoshop.exe")
        full_path
            .rsplit('\\')
            .next()
            .map(|s| s.to_lowercase())
    }
}

/// Check if the foreground app is in the pause list.
pub fn should_pause(pause_apps: &[String]) -> bool {
    match get_foreground_process_name() {
        Some(name) => pause_apps.iter().any(|app| app.to_lowercase() == name),
        None => false,
    }
}
