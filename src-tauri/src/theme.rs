//! Windows dark/light theme switching via registry.
//!
//! Sets `AppsUseLightTheme` and `SystemUsesLightTheme` under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`.
//! Windows and most apps pick up the change immediately.

use std::ffi::c_void;

// Registry FFI from advapi32.dll / kernel32.dll
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

// Constants
const HKEY_CURRENT_USER: *mut c_void = 0x80000001 as *mut c_void;
const KEY_SET_VALUE: u32 = 0x0002;
const KEY_QUERY_VALUE: u32 = 0x0001;
const REG_DWORD: u32 = 4;
const ERROR_SUCCESS: i32 = 0;

const PERSONALIZE_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

/// Encode a Rust string as a null-terminated UTF-16 vector.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Set the Windows theme to dark (true) or light (false).
///
/// Modifies both `AppsUseLightTheme` and `SystemUsesLightTheme`.
/// Returns true on success.
pub fn set_dark_theme(dark: bool) -> bool {
    let value: u32 = if dark { 0 } else { 1 };

    let sub_key = to_wide(PERSONALIZE_KEY);
    let mut hkey: *mut c_void = std::ptr::null_mut();

    unsafe {
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );

        if result != ERROR_SUCCESS {
            eprintln!("[lum] Failed to open Personalize registry key (error {result})");
            return false;
        }

        let apps_value = to_wide("AppsUseLightTheme");
        let system_value = to_wide("SystemUsesLightTheme");
        let data = value.to_le_bytes();

        let r1 = RegSetValueExW(
            hkey,
            apps_value.as_ptr(),
            0,
            REG_DWORD,
            data.as_ptr(),
            4,
        );

        let r2 = RegSetValueExW(
            hkey,
            system_value.as_ptr(),
            0,
            REG_DWORD,
            data.as_ptr(),
            4,
        );

        RegCloseKey(hkey);

        if r1 != ERROR_SUCCESS || r2 != ERROR_SUCCESS {
            eprintln!("[lum] Failed to write theme registry values (apps={r1}, system={r2})");
            return false;
        }
    }

    eprintln!("[lum] Theme set to {}", if dark { "dark" } else { "light" });
    true
}

/// Get the current theme state. Returns true if dark mode is active.
pub fn is_dark_theme() -> bool {
    let sub_key = to_wide(PERSONALIZE_KEY);
    let mut hkey: *mut c_void = std::ptr::null_mut();

    unsafe {
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut hkey,
        );

        if result != ERROR_SUCCESS {
            return false; // Can't read → assume light
        }

        let apps_value = to_wide("AppsUseLightTheme");
        let mut data: u32 = 1;
        let mut size: u32 = 4;
        let mut reg_type: u32 = 0;

        let r = RegQueryValueExW(
            hkey,
            apps_value.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            &mut data as *mut u32 as *mut u8,
            &mut size,
        );

        RegCloseKey(hkey);

        if r == ERROR_SUCCESS {
            data == 0 // 0 = dark mode active
        } else {
            false
        }
    }
}

/// Toggle the theme. Returns the new state (true = dark).
pub fn toggle_theme() -> bool {
    let currently_dark = is_dark_theme();
    let new_dark = !currently_dark;
    set_dark_theme(new_dark);
    new_dark
}
