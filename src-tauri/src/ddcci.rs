//! DDC/CI monitor brightness/contrast control via dxva2.dll.
//! Enumerates physical monitors and provides get/set for brightness (VCP 0x10)
//! and contrast (VCP 0x12).

use std::ffi::c_void;
use std::sync::Mutex;

// --- user32.dll: monitor enumeration ---
extern "system" {
    fn EnumDisplayMonitors(
        hdc: *mut c_void,
        lprcClip: *const c_void,
        lpfnEnum: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut RECT, isize) -> i32,
        dwData: isize,
    ) -> i32;
}

// --- dxva2.dll: DDC/CI physical monitor control ---
extern "system" {
    fn GetNumberOfPhysicalMonitorsFromHMONITOR(
        hMonitor: *mut c_void,
        pdwNumberOfPhysicalMonitors: *mut u32,
    ) -> i32;

    fn GetPhysicalMonitorsFromHMONITOR(
        hMonitor: *mut c_void,
        dwPhysicalMonitorArraySize: u32,
        pPhysicalMonitorArray: *mut PhysicalMonitor,
    ) -> i32;

    fn GetMonitorBrightness(
        hMonitor: *mut c_void,
        pdwMinimumBrightness: *mut u32,
        pdwCurrentBrightness: *mut u32,
        pdwMaximumBrightness: *mut u32,
    ) -> i32;

    fn SetMonitorBrightness(hMonitor: *mut c_void, dwNewBrightness: u32) -> i32;

    fn GetMonitorContrast(
        hMonitor: *mut c_void,
        pdwMinimumContrast: *mut u32,
        pdwCurrentContrast: *mut u32,
        pdwMaximumContrast: *mut u32,
    ) -> i32;

    fn SetMonitorContrast(hMonitor: *mut c_void, dwNewContrast: u32) -> i32;

    fn DestroyPhysicalMonitors(
        dwPhysicalMonitorArraySize: u32,
        pPhysicalMonitorArray: *mut PhysicalMonitor,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone)]
struct PhysicalMonitor {
    handle: *mut c_void,
    description: [u16; 128],
}

/// Info about a detected physical monitor.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Index in our internal list.
    pub index: usize,
    /// Description from the driver (may be empty).
    pub description: String,
    /// Whether DDC/CI brightness control is supported.
    pub supports_brightness: bool,
    /// Whether DDC/CI contrast control is supported.
    pub supports_contrast: bool,
    /// Min/max brightness range.
    pub brightness_min: u32,
    pub brightness_max: u32,
}

/// A handle to a physical monitor for DDC/CI operations.
struct MonitorHandle {
    handle: *mut c_void,
    info: MonitorInfo,
}

// MonitorHandle is only used within a single thread context
unsafe impl Send for MonitorHandle {}

/// Global cache of discovered monitors (protected by Mutex).
static MONITORS: Mutex<Option<Vec<MonitorHandle>>> = Mutex::new(None);

/// Enumerate all physical monitors and probe DDC/CI capabilities.
/// Call once at startup. Results are cached.
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    let mut hmonitors: Vec<*mut c_void> = Vec::new();

    // Collect HMONITOR handles
    unsafe extern "system" fn enum_proc(
        hMonitor: *mut c_void,
        _hdc: *mut c_void,
        _rect: *mut RECT,
        data: isize,
    ) -> i32 {
        let monitors = &mut *(data as *mut Vec<*mut c_void>);
        monitors.push(hMonitor);
        1 // Continue enumeration
    }

    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            enum_proc,
            &mut hmonitors as *mut Vec<*mut c_void> as isize,
        );
    }

    let mut monitor_handles: Vec<MonitorHandle> = Vec::new();
    let mut infos: Vec<MonitorInfo> = Vec::new();
    let mut index = 0;

    for hmon in &hmonitors {
        let mut count: u32 = 0;
        let ok = unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(*hmon, &mut count) };
        if ok == 0 || count == 0 {
            continue;
        }

        let mut phys_monitors: Vec<PhysicalMonitor> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            phys_monitors.push(PhysicalMonitor {
                handle: std::ptr::null_mut(),
                description: [0; 128],
            });
        }

        let ok = unsafe {
            GetPhysicalMonitorsFromHMONITOR(*hmon, count, phys_monitors.as_mut_ptr())
        };
        if ok == 0 {
            continue;
        }

        for pm in phys_monitors.iter() {
            // Probe brightness support
            let mut min_b: u32 = 0;
            let mut cur_b: u32 = 0;
            let mut max_b: u32 = 0;
            let brightness_ok =
                unsafe { GetMonitorBrightness(pm.handle, &mut min_b, &mut cur_b, &mut max_b) };

            // Probe contrast support
            let mut min_c: u32 = 0;
            let mut cur_c: u32 = 0;
            let mut max_c: u32 = 0;
            let contrast_ok =
                unsafe { GetMonitorContrast(pm.handle, &mut min_c, &mut cur_c, &mut max_c) };

            // Extract description
            let desc_len = pm.description.iter().position(|&c| c == 0).unwrap_or(128);
            let description = String::from_utf16_lossy(&pm.description[..desc_len]);

            let info = MonitorInfo {
                index,
                description,
                supports_brightness: brightness_ok != 0,
                supports_contrast: contrast_ok != 0,
                brightness_min: min_b,
                brightness_max: max_b,
            };

            monitor_handles.push(MonitorHandle {
                handle: pm.handle,
                info: info.clone(),
            });
            infos.push(info);
            index += 1;
        }

        // Note: we don't call DestroyPhysicalMonitors here because we keep handles alive
        // for later SetMonitorBrightness calls. They'll be destroyed on app exit.
    }

    *MONITORS.lock().unwrap() = Some(monitor_handles);

    eprintln!(
        "[lum] DDC/CI: found {} monitor(s), {} with brightness control",
        infos.len(),
        infos.iter().filter(|i| i.supports_brightness).count()
    );

    infos
}

/// Get cached monitor info (call enumerate_monitors first).
pub fn get_monitors() -> Vec<MonitorInfo> {
    MONITORS
        .lock()
        .unwrap()
        .as_ref()
        .map(|ms| ms.iter().map(|m| m.info.clone()).collect())
        .unwrap_or_default()
}

/// Set brightness (0–100%) on all DDC/CI-capable monitors.
/// Scales the percentage to each monitor's min/max range.
pub fn set_all_brightness(percent: u8) -> bool {
    let percent = percent.clamp(0, 100) as f64 / 100.0;
    let mut any_success = false;

    let monitors = MONITORS.lock().unwrap();
    if let Some(monitors) = monitors.as_ref() {
        for m in monitors {
            if !m.info.supports_brightness {
                continue;
            }
            let range = m.info.brightness_max.saturating_sub(m.info.brightness_min);
            let target = m.info.brightness_min + (range as f64 * percent).round() as u32;
            let ok = unsafe { SetMonitorBrightness(m.handle, target) };
            if ok != 0 {
                any_success = true;
            } else {
                eprintln!(
                    "[lum] DDC/CI: SetMonitorBrightness failed for monitor {}",
                    m.info.index
                );
            }
        }
    }

    any_success
}

/// Cleanup: destroy all physical monitor handles. Call on app exit.
pub fn cleanup() {
    let mut monitors = MONITORS.lock().unwrap();
    if let Some(ms) = monitors.take() {
        let mut phys: Vec<PhysicalMonitor> = ms
            .iter()
            .map(|m| PhysicalMonitor {
                handle: m.handle,
                description: [0; 128],
            })
            .collect();
        if !phys.is_empty() {
            unsafe {
                DestroyPhysicalMonitors(phys.len() as u32, phys.as_mut_ptr());
            }
        }
    }
}
