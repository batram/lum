//! Runtime-configurable global hotkeys backed by the Windows RegisterHotKey API.

use crate::config::HotkeyConfig;
use crate::engine::FadeEngine;
use crate::{gamma, theme};
use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

extern "system" {
    fn RegisterHotKey(hwnd: *mut c_void, id: i32, fs_modifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostThreadMessageW(thread_id: u32, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn GetCurrentThreadId() -> u32;
}

#[repr(C)]
struct Msg {
    hwnd: *mut c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

const WM_HOTKEY: u32 = 0x0312;
const WM_RELOAD_HOTKEYS: u32 = 0x8001;
const MOD_ALT: u32 = 0x0001;
const MOD_CTRL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

const ID_PAUSE: i32 = 1;
const ID_BRIGHTER: i32 = 2;
const ID_DARKER: i32 = 3;
const ID_THEME: i32 = 4;
const ID_DAY_NIGHT: i32 = 5;
const ID_BOOST: i32 = 6;
const IDS: [i32; 6] = [
    ID_PAUSE,
    ID_BRIGHTER,
    ID_DARKER,
    ID_THEME,
    ID_DAY_NIGHT,
    ID_BOOST,
];

pub struct HotkeyManager {
    thread_id: AtomicU32,
    generation: AtomicU64,
    pending: Mutex<HotkeyConfig>,
}

impl HotkeyManager {
    pub fn start(engine: Arc<FadeEngine>, config: HotkeyConfig) -> Arc<Self> {
        let manager = Arc::new(Self {
            thread_id: AtomicU32::new(0),
            generation: AtomicU64::new(1),
            pending: Mutex::new(config),
        });
        let listener = manager.clone();
        thread::spawn(move || listener.run(engine));
        manager
    }

    pub fn update(&self, config: HotkeyConfig) {
        *self.pending.lock().unwrap() = config;
        self.generation.fetch_add(1, Ordering::Release);
        let thread_id = self.thread_id.load(Ordering::Acquire);
        if thread_id != 0 {
            unsafe {
                PostThreadMessageW(thread_id, WM_RELOAD_HOTKEYS, 0, 0);
            }
        }
    }

    fn run(&self, engine: Arc<FadeEngine>) {
        unsafe {
            self.thread_id
                .store(GetCurrentThreadId(), Ordering::Release);
        }
        let mut applied_generation = 0;
        self.reload(&mut applied_generation);

        let mut msg = Msg {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wparam: 0,
            lparam: 0,
            time: 0,
            pt_x: 0,
            pt_y: 0,
        };
        loop {
            let result = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if result <= 0 {
                break;
            }
            if msg.message == WM_RELOAD_HOTKEYS {
                self.reload(&mut applied_generation);
                continue;
            }
            if msg.message == WM_HOTKEY {
                handle_action(msg.wparam as i32, &engine);
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        unregister_all();
    }

    fn reload(&self, applied_generation: &mut u64) {
        let generation = self.generation.load(Ordering::Acquire);
        if generation == *applied_generation {
            return;
        }
        unregister_all();
        let config = self.pending.lock().unwrap().clone();
        for (id, label, shortcut) in bindings(&config) {
            if shortcut.trim().is_empty() {
                continue;
            }
            match parse_hotkey(shortcut) {
                Ok((modifiers, key)) => {
                    let registered =
                        unsafe { RegisterHotKey(std::ptr::null_mut(), id, modifiers, key) != 0 };
                    if registered {
                        eprintln!("[lum][hotkeys] {label}: {shortcut}");
                    } else {
                        eprintln!("[lum][hotkeys] could not register {label}: {shortcut}");
                    }
                }
                Err(error) => eprintln!("[lum][hotkeys] invalid {label}: {error}"),
            }
        }
        *applied_generation = generation;
    }
}

fn bindings(config: &HotkeyConfig) -> [(i32, &'static str, &str); 6] {
    [
        (ID_PAUSE, "Toggle pause", &config.toggle_pause),
        (ID_BRIGHTER, "Brighter", &config.brighter),
        (ID_DARKER, "Darker", &config.darker),
        (ID_THEME, "Toggle theme", &config.toggle_theme),
        (ID_DAY_NIGHT, "Toggle day/night", &config.toggle_day_night),
        (ID_BOOST, "Boost", &config.boost),
    ]
}

pub fn validate_config(config: &HotkeyConfig) -> Result<(), String> {
    let mut used = HashSet::new();
    for (_, label, shortcut) in bindings(config) {
        let shortcut = shortcut.trim();
        if shortcut.is_empty() {
            continue;
        }
        let parsed = parse_hotkey(shortcut).map_err(|error| format!("{label}: {error}"))?;
        if !used.insert(parsed) {
            return Err(format!("{label}: shortcut is already assigned"));
        }
    }
    Ok(())
}

pub fn parse_hotkey(shortcut: &str) -> Result<(u32, u32), String> {
    let parts: Vec<_> = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("enter a shortcut or leave the field empty".into());
    }

    let mut modifiers = 0;
    for modifier in &parts[..parts.len() - 1] {
        match modifier.to_ascii_lowercase().as_str() {
            "alt" => modifiers |= MOD_ALT,
            "ctrl" | "control" => modifiers |= MOD_CTRL,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "windows" | "meta" => modifiers |= MOD_WIN,
            _ => return Err(format!("unknown modifier ‘{modifier}’")),
        }
    }
    let key_name = parts.last().unwrap().to_ascii_uppercase();
    let key = match key_name.as_str() {
        "UP" => 0x26,
        "DOWN" => 0x28,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "PAUSE" => 0x13,
        "SPACE" => 0x20,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" | "PAGE UP" => 0x21,
        "PAGEDOWN" | "PAGE DOWN" => 0x22,
        "NUMPAD0" => 0x60,
        "NUMPAD1" => 0x61,
        "NUMPAD2" => 0x62,
        "NUMPAD3" => 0x63,
        "NUMPAD4" => 0x64,
        "NUMPAD5" => 0x65,
        "NUMPAD6" => 0x66,
        "NUMPAD7" => 0x67,
        "NUMPAD8" => 0x68,
        "NUMPAD9" => 0x69,
        "NUMPADMULTIPLY" => 0x6a,
        "NUMPADADD" => 0x6b,
        "NUMPADSUBTRACT" => 0x6d,
        "NUMPADDECIMAL" => 0x6e,
        "NUMPADDIVIDE" => 0x6f,
        value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => {
            value.as_bytes()[0] as u32
        }
        value if value.starts_with('F') => value[1..]
            .parse::<u32>()
            .ok()
            .filter(|number| (1..=24).contains(number))
            .map(|number| 0x70 + number - 1)
            .ok_or_else(|| "function keys must be F1 through F24".to_string())?,
        _ => return Err(format!("unsupported key ‘{}’", parts.last().unwrap())),
    };
    if modifiers == 0 && !key_name.starts_with("NUMPAD") {
        return Err("include Ctrl, Alt, Shift, or Win (numpad keys may be used alone)".into());
    }
    Ok((modifiers, key))
}

fn unregister_all() {
    for id in IDS {
        unsafe {
            UnregisterHotKey(std::ptr::null_mut(), id);
        }
    }
}

fn handle_action(id: i32, engine: &FadeEngine) {
    match id {
        ID_PAUSE => {
            engine.toggle_pause();
        }
        ID_BRIGHTER => engine.step_brightness(5),
        ID_DARKER => engine.step_brightness(-5),
        ID_THEME => {
            theme::toggle_theme();
        }
        ID_DAY_NIGHT => {
            let state = engine.get_state();
            engine.jump_to(state.phase == "day" || state.phase == "morning");
        }
        ID_BOOST => {
            gamma::reset_gamma();
        }
        _ => return,
    }
    eprintln!("[lum][hotkeys] action {id}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_function_keys() {
        assert_eq!(
            parse_hotkey("Ctrl+Shift+F12"),
            Ok((MOD_CTRL | MOD_SHIFT, 0x7b))
        );
        assert_eq!(parse_hotkey("Alt+Up"), Ok((MOD_ALT, 0x26)));
    }

    #[test]
    fn rejects_bare_and_duplicate_shortcuts() {
        assert!(parse_hotkey("F5").is_err());
        let mut config = HotkeyConfig::default();
        config.boost = config.toggle_theme.clone();
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn allows_bare_numpad_keys() {
        assert_eq!(parse_hotkey("Numpad1"), Ok((0, 0x61)));
        assert_eq!(parse_hotkey("NumpadSubtract"), Ok((0, 0x6d)));
        assert!(parse_hotkey("1").is_err());
    }
}
