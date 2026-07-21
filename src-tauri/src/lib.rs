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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
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
    hardware_offset_pct: i16,
    overlay_offset_pct: i16,
    temperature_offset_k: i32,
) {
    engine.set_adjustments(
        hardware_offset_pct,
        overlay_offset_pct,
        temperature_offset_k,
    );
}

#[tauri::command]
fn reset_temporary_adjustments(engine: State<'_, Arc<FadeEngine>>) {
    engine.reset_adjustments();
}

#[tauri::command]
fn set_schedule_preview(engine: State<'_, Arc<FadeEngine>>, minute: Option<u16>) {
    engine.set_preview_minute(minute);
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
    let physical = ddcci::get_monitors();
    gamma::get_display_names()
        .into_iter()
        .enumerate()
        .map(|(index, device_name)| {
            let matches: Vec<_> = physical
                .iter()
                .filter(|monitor| monitor.device_name.eq_ignore_ascii_case(&device_name))
                .collect();
            let description = matches
                .iter()
                .find(|monitor| !monitor.description.is_empty())
                .map(|monitor| monitor.description.as_str())
                .unwrap_or(&device_name);
            serde_json::json!({
                "index": index,
                "device_name": device_name,
                "description": description,
                "supports_brightness": matches.iter().any(|monitor| monitor.supports_brightness),
                "supports_contrast": matches.iter().any(|monitor| monitor.supports_contrast),
            })
        })
        .collect()
}

/// Tauri command: get full settings as JSON.
#[tauri::command]
fn get_settings() -> config::Settings {
    config::Settings::load()
}

/// Tauri command: save settings from JSON.
#[tauri::command]
fn save_settings(
    settings: config::Settings,
    hotkey_manager: State<'_, Arc<hotkeys::HotkeyManager>>,
) -> Result<bool, String> {
    hotkeys::validate_config(&settings.hotkeys)?;
    settings.save();
    hotkey_manager.update(settings.hotkeys.clone());
    Ok(true)
}

pub fn run() {
    // Initialize engine and settings
    let fade_engine = FadeEngine::new();
    let settings = config::Settings::load();
    let hotkey_manager =
        hotkeys::HotkeyManager::start(fade_engine.clone(), settings.hotkeys.clone());
    let tray_interaction = Arc::new(AtomicBool::new(false));
    let tray_interaction_for_setup = tray_interaction.clone();
    let last_focus_dismissal = Arc::new(Mutex::new(None::<Instant>));
    let last_focus_dismissal_for_setup = last_focus_dismissal.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(fade_engine.clone())
        .manage(settings)
        .manage(hotkey_manager)
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            set_automatic,
            set_temporary_adjustments,
            reset_temporary_adjustments,
            set_schedule_preview,
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
            get_settings,
            save_settings,
        ])
        .setup(move |app| {
            // Ensure window stays hidden on startup (WebView2 may flash on nav errors)
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
            }

            // Enumerate DDC/CI monitors before the first engine tick so display
            // exclusions can restore hardware brightness immediately.
            ddcci::enumerate_monitors(&gamma::get_display_names());

            // Start the fade engine tick loop
            let engine_handle = Arc::new(Mutex::new(Some(fade_engine.start())));

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
            let engine_handle_for_tray = engine_handle.clone();
            let tray_click_generation = Arc::new(AtomicU64::new(0));
            let click_generation_for_tray = tray_click_generation.clone();
            let suppress_click_up_for_tray = Arc::new(AtomicBool::new(false));
            let tray_interaction_for_tray = tray_interaction_for_setup.clone();
            let last_focus_dismissal_for_tray = last_focus_dismissal_for_setup.clone();
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
                        // Stop the engine before cleanup so it cannot reapply
                        // display settings while the application is exiting.
                        engine_for_tray.stop();
                        if let Some(handle) = engine_handle_for_tray.lock().unwrap().take() {
                            let _ = handle.join();
                        }
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
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Down,
                            ..
                        } => {
                            eprintln!("[lum][tray] left down");
                            // The tray temporarily takes focus from the panel. Keep it visible
                            // until we know whether this interaction is a single or double click.
                            tray_interaction_for_tray.store(true, Ordering::Relaxed);
                        }
                        TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        } => {
                            suppress_click_up_for_tray.store(true, Ordering::Relaxed);
                            let behavior = config::Settings::load().developer.tray_click_behavior;
                            if behavior == config::TrayClickBehavior::Immediate {
                                eprintln!("[lum][tray] double click ignored in immediate mode");
                            } else {
                                eprintln!("[lum][tray] double click -> settings");
                                // Cancel a pending Windows-timed single-click action.
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
                            tray_interaction_for_tray.store(false, Ordering::Relaxed);
                        }
                        // Single left-click toggles the compact quick panel.
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            position,
                            ..
                        } => {
                            // Some Windows/Tauri combinations only surface the Up event, so this
                            // cannot rely on observing MouseButtonState::Down first.
                            tray_interaction_for_tray.store(true, Ordering::Relaxed);
                            // Windows emits a trailing Click(Up) for a double click. Consuming it
                            // prevents the quick panel from opening over the settings window.
                            if suppress_click_up_for_tray.swap(false, Ordering::Relaxed) {
                                eprintln!("[lum][tray] consumed double-click trailing up");
                                tray_interaction_for_tray.store(false, Ordering::Relaxed);
                                return;
                            }
                            let behavior = config::Settings::load().developer.tray_click_behavior;
                            let delay_ms = if behavior == config::TrayClickBehavior::WindowsTimed {
                                unsafe { GetDoubleClickTime() }.max(200)
                            } else {
                                0
                            };
                            let recently_dismissed_by_focus = last_focus_dismissal_for_tray
                                .lock()
                                .ok()
                                .and_then(|mut dismissed_at| dismissed_at.take())
                                .is_some_and(|dismissed_at| {
                                    dismissed_at.elapsed()
                                        <= Duration::from_millis(delay_ms as u64 + 250)
                                });
                            if recently_dismissed_by_focus {
                                eprintln!(
                                    "[lum][tray] left up consumed as close after focus dismissal"
                                );
                                tray_interaction_for_tray.store(false, Ordering::Relaxed);
                                return;
                            }
                            eprintln!("[lum][tray] left up -> schedule toggle in {delay_ms}ms");
                            // Windows reports the first click before it reports a double-click.
                            // Delay this action for the configured double-click interval so the
                            // double-click handler can cancel it without flashing the popup.
                            let generation =
                                click_generation_for_tray.fetch_add(1, Ordering::Relaxed) + 1;
                            let generation_guard = click_generation_for_tray.clone();
                            let app = app.clone();
                            let tray_interaction = tray_interaction_for_tray.clone();
                            thread::spawn(move || {
                                thread::sleep(Duration::from_millis(delay_ms as u64));
                                if generation_guard.load(Ordering::Relaxed) != generation {
                                    tray_interaction.store(false, Ordering::Relaxed);
                                    return;
                                }
                                let app_for_main = app.clone();
                                let _ = app.run_on_main_thread(move || {
                                    if generation_guard.load(Ordering::Relaxed) != generation {
                                        tray_interaction.store(false, Ordering::Relaxed);
                                        return;
                                    }
                                    if let Some(win) = app_for_main.get_webview_window("main") {
                                        if win.is_visible().unwrap_or(false) {
                                            eprintln!("[lum][tray] delayed toggle -> hide panel");
                                            let _ = win.hide();
                                        } else {
                                            eprintln!("[lum][tray] delayed toggle -> show panel");
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
                                    tray_interaction.store(false, Ordering::Relaxed);
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
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            if window.label() == "main" {
                if let tauri::WindowEvent::Focused(false) = event {
                    if !config::Settings::load().developer.close_on_focus_loss {
                        eprintln!("[lum][panel] focus lost; dismissal disabled");
                        return;
                    }
                    eprintln!("[lum][panel] focus lost");
                    // Windows can report focus loss before it delivers the tray mouse-down.
                    // Defer dismissal briefly so the tray handler can mark the interaction;
                    // otherwise the delayed single-click toggle sees a hidden panel and reopens it.
                    let app = window.app_handle().clone();
                    let tray_interaction = tray_interaction.clone();
                    let last_focus_dismissal = last_focus_dismissal.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(50));
                        let app_for_main = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            if tray_interaction.load(Ordering::Relaxed) {
                                eprintln!(
                                    "[lum][panel] focus dismissal skipped for tray interaction"
                                );
                                return;
                            }
                            if let Some(panel) = app_for_main.get_webview_window("main") {
                                if !panel.is_focused().unwrap_or(false) {
                                    if panel.is_visible().unwrap_or(false) {
                                        if let Ok(mut dismissed_at) = last_focus_dismissal.lock() {
                                            *dismissed_at = Some(Instant::now());
                                        }
                                    }
                                    eprintln!("[lum][panel] focus dismissal -> hide panel");
                                    let _ = panel.hide();
                                }
                            }
                        });
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Lum");
}
