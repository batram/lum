mod appdetect;
mod autostart;
mod color;
mod config;
mod ddcci;
mod engine;
mod gamma;
mod hotkeys;
mod sun;
mod theme;

use engine::FadeEngine;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State,
};

/// Tauri command: get current engine state for the frontend.
#[tauri::command]
fn get_app_state(engine: State<'_, Arc<FadeEngine>>) -> engine::EngineState {
    engine.get_state()
}

/// Tauri command: toggle pause.
#[tauri::command]
fn toggle_pause(engine: State<'_, Arc<FadeEngine>>) -> bool {
    engine.toggle_pause()
}

/// Tauri command: jump to day or night.
#[tauri::command]
fn jump_to_night(engine: State<'_, Arc<FadeEngine>>, night: bool) {
    engine.jump_to(night);
}

/// Tauri command: get today's sun times.
#[tauri::command]
fn get_sun_times(latitude: f64, longitude: f64) -> serde_json::Value {
    let sun = sun::calculate_today(latitude.clamp(-90.0, 90.0), longitude.clamp(-180.0, 180.0));
    use chrono::Timelike;
    serde_json::json!({
        "sunrise": format!("{:02}:{:02}", sun.sunrise.time().hour(), sun.sunrise.time().minute()),
        "sunset": format!("{:02}:{:02}", sun.sunset.time().hour(), sun.sunset.time().minute()),
        "civil_dawn": format!("{:02}:{:02}", sun.civil_dawn.time().hour(), sun.civil_dawn.time().minute()),
        "civil_dusk": format!("{:02}:{:02}", sun.civil_dusk.time().hour(), sun.civil_dusk.time().minute()),
    })
}

/// Tauri command: toggle Windows dark/light theme. Returns new state (true=dark).
#[tauri::command]
fn toggle_theme() -> bool {
    theme::toggle_theme()
}

/// Tauri command: set theme explicitly.
#[tauri::command]
fn set_theme(dark: bool) -> bool {
    theme::set_dark_theme(dark)
}

/// Tauri command: get current theme state (true=dark).
#[tauri::command]
fn get_theme() -> bool {
    theme::is_dark_theme()
}

/// Tauri command: toggle auto-start with Windows.
#[tauri::command]
fn toggle_autostart() -> bool {
    autostart::toggle_autostart()
}

/// Tauri command: check if autostart is enabled.
#[tauri::command]
fn get_autostart() -> bool {
    autostart::is_autostart_enabled()
}

/// Tauri command: get the foreground process name (for debugging pause list).
#[tauri::command]
fn get_foreground_app() -> Option<String> {
    appdetect::get_foreground_process_name()
}

/// Tauri command: get list of monitors with DDC/CI capabilities.
#[tauri::command]
fn get_monitors() -> Vec<serde_json::Value> {
    ddcci::get_monitors()
        .iter()
        .map(|m| {
            serde_json::json!({
                "index": m.index,
                "description": m.description,
                "supports_brightness": m.supports_brightness,
                "supports_contrast": m.supports_contrast,
                "brightness_min": m.brightness_min,
                "brightness_max": m.brightness_max,
            })
        })
        .collect()
}

/// Tauri command: set brightness on all monitors (0-100).
#[tauri::command]
fn set_brightness(percent: u8) -> bool {
    ddcci::set_all_brightness(percent)
}

/// Tauri command: set brightness on a specific monitor.
#[tauri::command]
fn set_monitor_brightness(index: usize, percent: u8) -> bool {
    ddcci::set_monitor_brightness(index, percent)
}

/// Tauri command: read current brightness from all monitors.
/// Returns a vec of Option<u8> (None if monitor doesn't support DDC/CI).
#[tauri::command]
fn get_all_brightness() -> Vec<Option<u8>> {
    let monitors = ddcci::get_monitors();
    monitors
        .iter()
        .map(|m| ddcci::get_monitor_brightness(m.index))
        .collect()
}

/// Tauri command: get full settings as JSON.
#[tauri::command]
fn get_settings() -> config::Settings {
    config::Settings::load()
}

/// Tauri command: save settings from JSON.
#[tauri::command]
fn save_settings(settings: config::Settings) -> bool {
    settings.save();
    true
}

pub fn run() {
    // Initialize engine and settings
    let fade_engine = FadeEngine::new();
    let settings = config::Settings::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(fade_engine.clone())
        .manage(settings)
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            toggle_pause,
            jump_to_night,
            get_sun_times,
            toggle_theme,
            set_theme,
            get_theme,
            toggle_autostart,
            get_autostart,
            get_foreground_app,
            get_monitors,
            set_brightness,
            set_monitor_brightness,
            get_all_brightness,
            get_settings,
            save_settings,
        ])
        .setup(move |app| {
            // Ensure window stays hidden on startup (WebView2 may flash on nav errors)
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
            }

            // Start the fade engine tick loop
            let _handle = fade_engine.start();

            // Start global hotkey listener
            let _hotkey_handle = hotkeys::start_hotkey_listener(fade_engine.clone());

            // Enumerate DDC/CI monitors
            ddcci::enumerate_monitors();

            // --- Tray menu ---
            let show_i = MenuItem::with_id(app, "show", "Show Lum", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
            let day_night_i =
                MenuItem::with_id(app, "day_night", "Jump to Night", true, None::<&str>)?;
            let theme_i = MenuItem::with_id(app, "theme", "Toggle Theme", true, None::<&str>)?;
            let boost_i =
                MenuItem::with_id(app, "boost", "Boost (full bright)", true, None::<&str>)?;
            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &show_i,
                    &pause_i,
                    &day_night_i,
                    &theme_i,
                    &boost_i,
                    &settings_i,
                    &quit_i,
                ],
            )?;

            // --- Tray icon ---
            let engine_for_tray = fade_engine.clone();
            let tray_icon = app.default_window_icon().cloned().unwrap();
            let _tray = TrayIconBuilder::with_id("lum-tray")
                .icon(tray_icon)
                .tooltip("Lum — starting...")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => {
                        // Reset gamma and cleanup DDC/CI before exiting
                        gamma::reset_gamma();
                        ddcci::cleanup();
                        app.exit(0);
                    }
                    "pause" => {
                        engine_for_tray.toggle_pause();
                    }
                    "day_night" => {
                        let state = engine_for_tray.get_state();
                        let go_night = state.phase == "day" || state.phase == "morning";
                        engine_for_tray.jump_to(go_night);
                    }
                    "theme" => {
                        theme::toggle_theme();
                    }
                    "boost" => {
                        // Boost = jump to full day brightness temporarily
                        engine_for_tray.jump_to(false);
                        println!("TODO: auto-resume after 2 min");
                    }
                    "settings" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Left-click tray icon → show/hide window
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        // Window close = hide to tray (not quit)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Lum");
}
