//! Fade engine: 1-second tick loop that calculates intensity from sun position,
//! applies the user's curve, and drives the rendering backend (gamma ramps or Night Light).

use crate::appdetect;
use crate::color::{kelvin_to_rgb, lerp_kelvin};
use crate::config::Settings;
use crate::ddcci;
use crate::gamma::{self, GammaRamps};
use crate::sun;
use crate::theme;
use chrono::Local;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Live state snapshot, exposed to the frontend via Tauri command.
#[derive(Debug, Clone, Serialize)]
pub struct EngineState {
    /// Current phase: "day", "evening", "morning", "night"
    pub phase: String,
    /// Current intensity 0.0 (day) → 1.0 (night)
    pub intensity: f64,
    /// Current color temperature in Kelvin
    pub color_temp_k: u32,
    /// Current brightness percentage (0–100)
    pub brightness_pct: u8,
    /// Whether the engine is paused
    pub paused: bool,
    /// Today's sunrise time (HH:MM)
    pub sunrise: String,
    /// Today's sunset time (HH:MM)
    pub sunset: String,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            phase: "day".to_string(),
            intensity: 0.0,
            color_temp_k: 6500,
            brightness_pct: 100,
            paused: false,
            sunrise: "--:--".to_string(),
            sunset: "--:--".to_string(),
        }
    }
}

/// Shared engine state, accessible from Tauri commands and the tick thread.
pub struct FadeEngine {
    pub state: std::sync::Mutex<EngineState>,
    pub paused: AtomicBool,
    /// Signal to stop the tick thread.
    stop: AtomicBool,
}

impl FadeEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: std::sync::Mutex::new(EngineState::default()),
            paused: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        })
    }

    /// Get a snapshot of the current state.
    pub fn get_state(&self) -> EngineState {
        self.state.lock().unwrap().clone()
    }

    /// Toggle pause. Returns new paused state.
    pub fn toggle_pause(&self) -> bool {
        let new_val = !self.paused.load(Ordering::Relaxed);
        self.paused.store(new_val, Ordering::Relaxed);
        if new_val {
            // When pausing, reset gamma to identity
            gamma::reset_gamma();
        }
        new_val
    }

    /// Force jump to day or night mode.
    pub fn jump_to(&self, night: bool) {
        let intensity = if night { 1.0 } else { 0.0 };
        let settings = Settings::load();
        let success = apply_effect(&settings, intensity);
        eprintln!(
            "[lum] jump_to({}) → intensity={}, gamma_ok={}",
            if night { "night" } else { "day" },
            intensity,
            success
        );

        let mut state = self.state.lock().unwrap();
        state.intensity = intensity;
        state.phase = if night { "night" } else { "day" }.to_string();
        state.color_temp_k = lerp_kelvin(
            settings.color.day_temp_k,
            settings.color.night_temp_k,
            intensity,
        );
        state.brightness_pct = lerp_brightness(&settings, intensity);
    }

    /// Start the background tick thread. Returns a handle to the thread.
    pub fn start(self: &Arc<Self>) -> thread::JoinHandle<()> {
        let engine = Arc::clone(self);
        thread::spawn(move || {
            // Cache sun times, recalculate once per hour
            let mut cached_sun: Option<sun::SunTimes> = None;
            let mut last_sun_calc: Option<chrono::NaiveDate> = None;
            let mut last_location: Option<(f64, f64)> = None;
            let mut tick_count: u64 = 0;
            let mut last_phase: String = String::new();

            while !engine.stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(1));
                tick_count += 1;

                let settings = Settings::load();

                // Skip processing if paused (manual or per-app)
                let app_paused = appdetect::should_pause(&settings.pause_apps);
                if engine.paused.load(Ordering::Relaxed) || app_paused {
                    // If app-pause just kicked in, reset gamma
                    if app_paused {
                        let state = engine.state.lock().unwrap();
                        if !state.paused {
                            drop(state);
                            gamma::reset_gamma();
                        }
                    }
                    let mut state = engine.state.lock().unwrap();
                    state.paused = true;
                    continue;
                }

                let now = Local::now().naive_local();
                let today = now.date();

                // Recalculate sun times at start of each new day or every 3600 ticks
                let location = (settings.location.latitude, settings.location.longitude);
                let location_changed = last_location
                    .map(|old| (old.0 - location.0).abs() > f64::EPSILON || (old.1 - location.1).abs() > f64::EPSILON)
                    .unwrap_or(true);
                let needs_recalc = last_sun_calc != Some(today) || location_changed || tick_count % 3600 == 1;
                if needs_recalc {
                    cached_sun = Some(sun::calculate_today(
                        settings.location.latitude,
                        settings.location.longitude,
                    ));
                    last_sun_calc = Some(today);
                    last_location = Some(location);
                }

                let calculated_sun = match &cached_sun {
                    Some(s) => s,
                    None => continue,
                };
                let mut effective_sun = calculated_sun.clone();
                if settings.fade.use_civil_twilight {
                    effective_sun.sunrise = calculated_sun.civil_dawn;
                    effective_sun.sunset = calculated_sun.civil_dusk;
                }

                // Calculate current intensity
                let (raw_intensity, phase) = sun::current_intensity(
                    &effective_sun,
                    now,
                    settings.fade.fade_duration_min,
                    settings.fade.evening_offset_min,
                    settings.fade.morning_offset_min,
                );

                // Apply curve (for now: linear — bezier editor comes later)
                let intensity = raw_intensity;

                // Apply the effect to the display
                apply_effect(&settings, intensity);

                // Auto-switch Windows theme on phase transitions
                if settings.theme.auto_switch && phase != last_phase {
                    let is_night_phase = phase == "night";
                    let was_night_phase = last_phase == "night";
                    if is_night_phase && !was_night_phase {
                        // Transitioned to night
                        theme::set_dark_theme(settings.theme.dark_at_night);
                    } else if !is_night_phase && was_night_phase {
                        // Transitioned from night to day
                        theme::set_dark_theme(!settings.theme.dark_at_night);
                    }
                }
                last_phase = phase.to_string();

                // Update shared state
                let color_temp = lerp_kelvin(
                    settings.color.day_temp_k,
                    settings.color.night_temp_k,
                    intensity,
                );
                let brightness = lerp_brightness(&settings, intensity);

                let mut state = engine.state.lock().unwrap();
                state.phase = phase.to_string();
                state.intensity = intensity;
                state.color_temp_k = color_temp;
                state.brightness_pct = brightness;
                state.paused = false;
                state.sunrise = format!("{:02}:{:02}", calculated_sun.sunrise.time().hour(), calculated_sun.sunrise.time().minute());
                state.sunset = format!("{:02}:{:02}", calculated_sun.sunset.time().hour(), calculated_sun.sunset.time().minute());
            }

            // Cleanup on stop: reset gamma
            gamma::reset_gamma();
        })
    }

    /// Signal the tick thread to stop.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Apply the visual effect (color temp + brightness) via the configured engine.
/// Returns true if the gamma ramp was applied successfully.
fn apply_effect(settings: &Settings, intensity: f64) -> bool {
    let color_temp = lerp_kelvin(settings.color.day_temp_k, settings.color.night_temp_k, intensity);
    let rgb = kelvin_to_rgb(color_temp);
    let brightness_pct = lerp_brightness(settings, intensity);
    let brightness = brightness_pct as f64 / 100.0;

    // Apply DDC/CI hardware brightness to physical monitors
    ddcci::set_all_brightness(brightness_pct);

    // Apply color temperature via gamma ramps (or Night Light)
    match settings.engine {
        crate::config::EngineKind::GammaRamps => {
            // Gamma ramps handle color only; brightness handled by DDC/CI above.
            // If DDC/CI unavailable, gamma also handles brightness.
            let ramps = GammaRamps::from_color_and_brightness(&rgb, brightness);
            gamma::set_gamma_ramp(&ramps)
        }
        crate::config::EngineKind::NightLight => {
            // TODO: Night Light registry control
            let ramps = GammaRamps::from_color_and_brightness(&rgb, brightness);
            gamma::set_gamma_ramp(&ramps)
        }
    }
}

/// Interpolate brightness percentage between day and night values.
fn lerp_brightness(settings: &Settings, intensity: f64) -> u8 {
    let day = settings.brightness.day_percent as f64;
    let night = settings.brightness.night_percent as f64;
    (day + (night - day) * intensity).round() as u8
}

// Need this import for time formatting in the tick loop
use chrono::Timelike;
