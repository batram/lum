//! Auto-start with Windows via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
//! No admin required. Portable-friendly.

use std::ffi::c_void;

extern "system" {
    fn RegOpenKeyExW(
        hKey: *mut c_void,
        lpSubKey: *const u16,
        ulOptions: u32,
        samDesired: u32,
        phkResult: *mut *mut c_void,
    ) -> i32;

    fn RegSetValueExW(
        hKey: *mut c_void,
        lpValueName: *const u16,
        Reserved: u32,
        dwType: u32,
        lpData: *const u8,
        cbData: u32,
    ) -> i32;

    fn RegDeleteValueW(hKey: *mut c_void, lpValueName: *const u16) -> i32;

    fn RegQueryValueExW(
        hKey: *mut c_void,
        lpValueName: *const u16,
        lpReserved: *mut u32,
        lpType: *mut u32,
        lpData: *mut u8,
        lpcbData: *mut u32,
    ) -> i32;

    fn RegCloseKey(hKey: *mut c_void) -> i32;
}

const HKEY_CURRENT_USER: *mut c_void = 0x80000001 as *mut c_void;
const KEY_SET_VALUE: u32 = 0x0002;
const KEY_QUERY_VALUE: u32 = 0x0001;
const REG_SZ: u32 = 1;
const ERROR_SUCCESS: i32 = 0;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "Lum";

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Register Lum to start with Windows. Uses the current exe path.
pub fn enable_autostart() -> bool {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("[lum] Cannot get exe path for autostart: {e}");
            return false;
        }
    };

    // Wrap in quotes in case path has spaces
    let command = format!("\"{}\"", exe_path);
    let command_wide = to_wide(&command);

    unsafe {
        let sub_key = to_wide(RUN_KEY);
        let mut hkey: *mut c_void = std::ptr::null_mut();

        let result = RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, KEY_SET_VALUE, &mut hkey);
        if result != ERROR_SUCCESS {
            eprintln!("[lum] Failed to open Run key (error {result})");
            return false;
        }

        let value_name = to_wide(APP_NAME);
        let r = RegSetValueExW(
            hkey,
            value_name.as_ptr(),
            0,
            REG_SZ,
            command_wide.as_ptr() as *const u8,
            (command_wide.len() * 2) as u32,
        );

        RegCloseKey(hkey);

        if r != ERROR_SUCCESS {
            eprintln!("[lum] Failed to set autostart value (error {r})");
            return false;
        }
    }

    eprintln!("[lum] Autostart enabled: {command}");
    true
}

/// Remove Lum from Windows startup.
pub fn disable_autostart() -> bool {
    unsafe {
        let sub_key = to_wide(RUN_KEY);
        let mut hkey: *mut c_void = std::ptr::null_mut();

        let result = RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, KEY_SET_VALUE, &mut hkey);
        if result != ERROR_SUCCESS {
            return false;
        }

        let value_name = to_wide(APP_NAME);
        let r = RegDeleteValueW(hkey, value_name.as_ptr());
        RegCloseKey(hkey);

        r == ERROR_SUCCESS
    }
}

/// Check if autostart is currently enabled.
pub fn is_autostart_enabled() -> bool {
    unsafe {
        let sub_key = to_wide(RUN_KEY);
        let mut hkey: *mut c_void = std::ptr::null_mut();

        let result = RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, KEY_QUERY_VALUE, &mut hkey);
        if result != ERROR_SUCCESS {
            return false;
        }

        let value_name = to_wide(APP_NAME);
        let mut buf = [0u8; 512];
        let mut size: u32 = buf.len() as u32;
        let mut reg_type: u32 = 0;

        let r = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            buf.as_mut_ptr(),
            &mut size,
        );

        RegCloseKey(hkey);
        r == ERROR_SUCCESS
    }
}

/// Toggle autostart. Returns new state (true = enabled).
pub fn toggle_autostart() -> bool {
    if is_autostart_enabled() {
        disable_autostart();
        false
    } else {
        enable_autostart();
        true
    }
}
