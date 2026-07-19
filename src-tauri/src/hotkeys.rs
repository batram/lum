//! Global hotkey registration via Windows RegisterHotKey API.
//! Runs a dedicated message loop thread to receive WM_HOTKEY messages.

use std::ffi::c_void;
use std::sync::Arc;
use std::thread;

use crate::engine::FadeEngine;
use crate::gamma;
use crate::theme;

extern "system" {
    fn RegisterHotKey(hwnd: *mut c_void, id: i32, fsModifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
    fn GetMessageW(lpMsg: *mut MSG, hWnd: *mut c_void, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn TranslateMessage(lpMsg: *const MSG) -> i32;
    fn DispatchMessageW(lpMsg: *const MSG) -> isize;
    fn PostThreadMessageW(idThread: u32, msg: u32, wParam: usize, lParam: isize) -> i32;
    fn GetCurrentThreadId() -> u32;
}

#[repr(C)]
struct MSG {
    hwnd: *mut c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

const WM_HOTKEY: u32 = 0x0312;
const WM_QUIT: u32 = 0x0012;

// Modifier keys
const MOD_ALT: u32 = 0x0001;
const MOD_CTRL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;

// Virtual key codes
const VK_PAUSE: u32 = 0x13;    // Pause/Break key
const VK_UP: u32 = 0x26;
const VK_DOWN: u32 = 0x28;
const VK_F5: u32 = 0x74;
const VK_F6: u32 = 0x75;
const VK_F7: u32 = 0x76;

// Hotkey IDs
const ID_PAUSE: i32 = 1;
const ID_DIM_UP: i32 = 2;
const ID_DIM_DOWN: i32 = 3;
const ID_THEME: i32 = 4;
const ID_DAY_NIGHT: i32 = 5;
const ID_BOOST: i32 = 6;

/// Default hotkey bindings:
/// - Alt+Pause  → toggle pause
/// - Alt+Up     → dim up (brighter)
/// - Alt+Down   → dim down (darker)
/// - Alt+F5     → toggle theme
/// - Alt+F6     → jump day/night
/// - Alt+F7     → boost (full bright)
pub fn start_hotkey_listener(engine: Arc<FadeEngine>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        unsafe {
            // Register hotkeys (hwnd=NULL → thread-level, delivered via message loop)
            RegisterHotKey(std::ptr::null_mut(), ID_PAUSE, MOD_ALT, VK_PAUSE);
            RegisterHotKey(std::ptr::null_mut(), ID_DIM_UP, MOD_ALT, VK_UP);
            RegisterHotKey(std::ptr::null_mut(), ID_DIM_DOWN, MOD_ALT, VK_DOWN);
            RegisterHotKey(std::ptr::null_mut(), ID_THEME, MOD_ALT, VK_F5);
            RegisterHotKey(std::ptr::null_mut(), ID_DAY_NIGHT, MOD_ALT, VK_F6);
            RegisterHotKey(std::ptr::null_mut(), ID_BOOST, MOD_ALT, VK_F7);

            eprintln!("[lum] Global hotkeys registered (Alt+Pause, Alt+↑/↓, Alt+F5/F6/F7)");

            // Message loop
            let mut msg = MSG {
                hwnd: std::ptr::null_mut(),
                message: 0,
                wparam: 0,
                lparam: 0,
                time: 0,
                pt_x: 0,
                pt_y: 0,
            };

            loop {
                let result = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                if result <= 0 {
                    break;
                }

                if msg.message == WM_HOTKEY {
                    match msg.wparam as i32 {
                        ID_PAUSE => {
                            let paused = engine.toggle_pause();
                            eprintln!("[lum] Hotkey: pause → {}", if paused { "paused" } else { "resumed" });
                        }
                        ID_DIM_UP => {
                            // Jump toward day (brighter)
                            engine.jump_to(false);
                            eprintln!("[lum] Hotkey: dim up (jump to day)");
                        }
                        ID_DIM_DOWN => {
                            // Jump toward night (darker/warmer)
                            engine.jump_to(true);
                            eprintln!("[lum] Hotkey: dim down (jump to night)");
                        }
                        ID_THEME => {
                            let dark = theme::toggle_theme();
                            eprintln!("[lum] Hotkey: theme → {}", if dark { "dark" } else { "light" });
                        }
                        ID_DAY_NIGHT => {
                            let state = engine.get_state();
                            let go_night = state.phase == "day" || state.phase == "morning";
                            engine.jump_to(go_night);
                            eprintln!("[lum] Hotkey: day/night jump → {}", if go_night { "night" } else { "day" });
                        }
                        ID_BOOST => {
                            // Boost: reset to full brightness (identity gamma)
                            gamma::reset_gamma();
                            eprintln!("[lum] Hotkey: boost (full bright, TODO: auto-resume 2min)");
                        }
                        _ => {}
                    }
                }

                if msg.message == WM_QUIT {
                    break;
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Cleanup
            UnregisterHotKey(std::ptr::null_mut(), ID_PAUSE);
            UnregisterHotKey(std::ptr::null_mut(), ID_DIM_UP);
            UnregisterHotKey(std::ptr::null_mut(), ID_DIM_DOWN);
            UnregisterHotKey(std::ptr::null_mut(), ID_THEME);
            UnregisterHotKey(std::ptr::null_mut(), ID_DAY_NIGHT);
            UnregisterHotKey(std::ptr::null_mut(), ID_BOOST);
        }
    })
}
