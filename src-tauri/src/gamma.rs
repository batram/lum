//! Gamma ramp control via Windows `SetDeviceGammaRamp` / `GetDeviceGammaRamp`.
//! Reprograms the GPU hardware LUT directly — zero performance cost, per-monitor.
//!
//! Uses CreateDCW with enumerated display devices for maximum driver compatibility
//! (GetDC(NULL) fails on many NVIDIA/AMD drivers).

use crate::color::RgbMultiplier;
use std::ffi::c_void;

// Raw FFI declarations from gdi32.dll / user32.dll
extern "system" {
    fn CreateDCW(lpszDriver: *const u16, lpszDevice: *const u16, lpszOutput: *const u16, lpInitData: *const c_void) -> *mut c_void;
    fn DeleteDC(hdc: *mut c_void) -> i32;
    fn SetDeviceGammaRamp(hdc: *mut c_void, lpRamp: *mut c_void) -> i32;
    fn EnumDisplayDevicesW(lpDevice: *const u16, iDevNum: u32, lpDisplayDevice: *mut DISPLAY_DEVICEW, dwFlags: u32) -> i32;
}

#[repr(C)]
#[derive(Clone)]
struct DISPLAY_DEVICEW {
    cb: u32,
    device_name: [u16; 32],
    device_string: [u16; 128],
    state_flags: u32,
    device_id: [u16; 128],
    device_key: [u16; 128],
}

impl DISPLAY_DEVICEW {
    fn new() -> Self {
        Self {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            device_name: [0; 32],
            device_string: [0; 128],
            state_flags: 0,
            device_id: [0; 128],
            device_key: [0; 128],
        }
    }

    /// Get the device name as a Rust string (e.g. "\\\\.\\DISPLAY1")
    fn name(&self) -> Vec<u16> {
        let len = self.device_name.iter().position(|&c| c == 0).unwrap_or(32);
        let mut name = self.device_name[..len].to_vec();
        name.push(0); // null terminator
        name
    }
}

// DISPLAY_DEVICE_ACTIVE = 0x1, DISPLAY_DEVICE_PRIMARY_DEVICE = 0x4
const DISPLAY_DEVICE_ACTIVE: u32 = 0x1;

/// A 256-entry gamma ramp for one channel.
pub type GammaRamp = [u16; 256];

/// Full RGB gamma ramp (3 × 256 entries).
#[derive(Clone)]
pub struct GammaRamps {
    pub red: GammaRamp,
    pub green: GammaRamp,
    pub blue: GammaRamp,
}

impl GammaRamps {
    /// Identity ramp (no modification — neutral 6500K, full brightness).
    pub fn identity() -> Self {
        let mut ramp = [0u16; 256];
        for (i, val) in ramp.iter_mut().enumerate() {
            *val = (i * 256) as u16; // 0, 256, 512, ... 65280
        }
        Self {
            red: ramp,
            green: ramp,
            blue: ramp,
        }
    }

    /// Build a gamma ramp from color multipliers and a brightness factor.
    pub fn from_color_and_brightness(color: &RgbMultiplier, brightness: f64) -> Self {
        let brightness = brightness.clamp(0.0, 1.0);

        let mut red = [0u16; 256];
        let mut green = [0u16; 256];
        let mut blue = [0u16; 256];

        for i in 0..256 {
            let base = i as f64 / 255.0;
            red[i] = (base * color.r * brightness * 65535.0).clamp(0.0, 65535.0) as u16;
            green[i] = (base * color.g * brightness * 65535.0).clamp(0.0, 65535.0) as u16;
            blue[i] = (base * color.b * brightness * 65535.0).clamp(0.0, 65535.0) as u16;
        }

        Self { red, green, blue }
    }

    /// Convert to the raw [[u16; 256]; 3] format expected by SetDeviceGammaRamp.
    pub fn as_raw(&self) -> [[u16; 256]; 3] {
        [self.red, self.green, self.blue]
    }
}

/// Enumerate all active display devices.
fn get_display_devices() -> Vec<Vec<u16>> {
    let mut devices = Vec::new();
    let mut i = 0u32;
    loop {
        let mut dd = DISPLAY_DEVICEW::new();
        let result = unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut dd, 0) };
        if result == 0 {
            break;
        }
        if dd.state_flags & DISPLAY_DEVICE_ACTIVE != 0 {
            devices.push(dd.name());
        }
        i += 1;
    }
    devices
}

/// Windows device names for all active logical displays.
pub fn get_display_names() -> Vec<String> {
    get_display_devices()
        .iter()
        .map(|name| {
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            String::from_utf16_lossy(&name[..len])
        })
        .collect()
}

fn is_excluded(name: &str, excluded: &[String]) -> bool {
    excluded.iter().any(|item| item.eq_ignore_ascii_case(name))
}

/// Apply a gamma ramp to ALL active displays.
///
/// Returns true if at least one display succeeded.
pub fn set_gamma_ramp(ramps: &GammaRamps) -> bool {
    set_gamma_ramp_except(ramps, &[])
}

/// Apply a gamma ramp to active displays, restoring excluded displays to identity.
pub fn set_gamma_ramp_except(ramps: &GammaRamps, excluded: &[String]) -> bool {
    let devices = get_display_devices();
    if devices.is_empty() {
        eprintln!("[lum] No active display devices found");
        return false;
    }

    let mut raw = ramps.as_raw();
    let mut identity = GammaRamps::identity().as_raw();
    let mut any_success = false;

    for device_name in &devices {
        let name_len = device_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(device_name.len());
        let name = String::from_utf16_lossy(&device_name[..name_len]);
        let selected_ramp = if is_excluded(&name, excluded) {
            &mut identity
        } else {
            &mut raw
        };
        unsafe {
            let hdc = CreateDCW(
                std::ptr::null(),
                device_name.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            );
            if hdc.is_null() {
                eprintln!("[lum] CreateDCW failed for a display");
                continue;
            }
            let result = SetDeviceGammaRamp(hdc, selected_ramp.as_mut_ptr() as *mut c_void);
            DeleteDC(hdc);
            if result != 0 {
                any_success = true;
            } else {
                eprintln!("[lum] SetDeviceGammaRamp failed for a display");
            }
        }
    }

    if !any_success {
        eprintln!("[lum] SetDeviceGammaRamp failed on ALL displays");
    }
    any_success
}

/// Reset gamma to identity (neutral — no color/brightness modification).
pub fn reset_gamma() -> bool {
    set_gamma_ramp(&GammaRamps::identity())
}

#[cfg(test)]
mod tests {
    use super::is_excluded;

    #[test]
    fn display_exclusions_are_case_insensitive() {
        let excluded = vec![r"\\.\DISPLAY2".to_string()];
        assert!(is_excluded(r"\\.\display2", &excluded));
        assert!(!is_excluded(r"\\.\DISPLAY1", &excluded));
    }
}
