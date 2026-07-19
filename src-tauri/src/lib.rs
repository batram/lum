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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, Position, State,
};

#[link(name = "user32")]
extern "system" {
    fn GetDoubleClickTime() -> u32;
}

/// Tauri command: get current engine state for the frontend.
#[tauri::command]
fn get_app_state(engine: State<'_, Arc<FadeEngine>>) -> engine::EngineState {
    engine.get_state()
}

#[tauri::command]
fn set_automatic(engine: State<'_, Arc<FadeEngine>>, automatic: bool) {
    engine.set_automatic(automatic);
}

#[tauri::command]
fn set_temporary_adjustments(
    engine: State<'_, Arc<FadeEngine>>,
    brightness_offset_pct: i16,
    temperature_offset_k: i32,
) {
    engine.set_adjustments(brightness_offset_pct, temperature_offset_k);
}

#[tauri::command]
fn reset_temporary_adjustments(engine: State<'_, Arc<FadeEngine>>) {
    engine.reset_adjustments();
}

#[tauri::command]
fn set_effects_off(engine: State<'_, Arc<FadeEngine>>, effects_off: bool) {
    engine.set_effects_off(effects_off);
}

#[tauri::command]
fn hide_quick_panel(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn open_settings_window(app: AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.hide();
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.center();
        let _ = window.set_focus();
    }
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
            set_automatic,
            set_temporary_adjustments,
            reset_temporary_adjustments,
            set_effects_off,
            hide_quick_panel,
            open_settings_window,
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
            let show_i = MenuItem::with_id(app, "show", "Quick controls", true, None::<&str>)?;
            let automatic_i =
                MenuItem::with_id(app, "automatic", "Toggle automatic", true, None::<&str>)?;
            let effects_i =
                MenuItem::with_id(app, "effects", "Turn effects off", true, None::<&str>)?;
            let theme_i = MenuItem::with_id(app, "theme", "Toggle Theme", true, None::<&str>)?;
            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &show_i,
                    &automatic_i,
                    &effects_i,
                    &theme_i,
                    &settings_i,
                    &quit_i,
                ],
            )?;

            // --- Tray icon ---
            let engine_for_tray = fade_engine.clone();
            let tray_click_generation = Arc::new(AtomicU64::new(0));
            let click_generation_for_tray = tray_click_generation.clone();
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
                    "automatic" => {
                        let state = engine_for_tray.get_state();
                        engine_for_tray.set_automatic(!state.automatic);
                    }
                    "effects" => {
                        let state = engine_for_tray.get_state();
                        engine_for_tray.set_effects_off(!state.effects_off);
                    }
                    "theme" => {
                        theme::toggle_theme();
                    }
                    "settings" => {
                        if let Some(panel) = app.get_webview_window("main") {
                            let _ = panel.hide();
                        }
                        if let Some(win) = app.get_webview_window("settings") {
                            let _ = win.show();
                            let _ = win.center();
                            let _ = win.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    let app = tray.app_handle();
                    match event {
                        // Double-click opens the full settings window.
                        TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        } => {
                            // Cancel the delayed single-click action before opening settings.
                            click_generation_for_tray.fetch_add(1, Ordering::Relaxed);
                            if let Some(panel) = app.get_webview_window("main") {
                                let _ = panel.hide();
                            }
                            if let Some(settings) = app.get_webview_window("settings") {
                                let _ = settings.show();
                                let _ = settings.center();
                                let _ = settings.set_focus();
                            }
                        }
                        // Single left-click toggles the compact quick panel.
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            position,
                            ..
                        } => {
                            // Windows reports the first click before it reports a double-click.
                            // Delay this action for the configured double-click interval so the
                            // double-click handler can cancel it without flashing the popup.
                            let generation = click_generation_for_tray
                                .fetch_add(1, Ordering::Relaxed)
                                + 1;
                            let generation_guard = click_generation_for_tray.clone();
                            let app = app.clone();
                            let delay_ms = unsafe { GetDoubleClickTime() }.max(200);
                            thread::spawn(move || {
                                thread::sleep(Duration::from_millis(delay_ms as u64));
                                if generation_guard.load(Ordering::Relaxed) != generation {
                                    return;
                                }
                                let app_for_main = app.clone();
                                let _ = app.run_on_main_thread(move || {
                                    if generation_guard.load(Ordering::Relaxed) != generation {
                                        return;
                                    }
                                    if let Some(win) = app_for_main.get_webview_window("main") {
                                        if win.is_visible().unwrap_or(false) {
                                            let _ = win.hide();
                                        } else {
                                            if let Ok(size) = win.outer_size() {
                                                let x = (position.x - size.width as f64 + 28.0)
                                                    .round()
                                                    as i32;
                                                let y = (position.y - size.height as f64 - 12.0)
                                                    .round()
                                                    as i32;
                                                let _ = win.set_position(Position::Physical(
                                                    PhysicalPosition::new(x, y),
                                                ));
                                            }
                                            let _ = win.show();
                                            let _ = win.set_focus();
                                        }
                                    }
                                });
                            });
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            Ok(())
        })
        // Both surfaces close to tray. The quick panel also dismisses on focus loss.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            if window.label() == "main" {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Lum");
}
