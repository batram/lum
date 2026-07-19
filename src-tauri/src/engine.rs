//! Solar schedule engine and runtime controls.

use crate::appdetect;
use crate::color::{kelvin_to_rgb, lerp_kelvin};
use crate::config::Settings;
use crate::ddcci;
use crate::gamma::{self, GammaRamps};
use crate::sun;
use crate::theme;
use chrono::{Duration as ChronoDuration, Local, NaiveDateTime, Timelike};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
struct RuntimeControls {
    automatic: bool,
    brightness_offset_pct: i16,
    temperature_offset_k: i32,
    offset_expires_at: Option<NaiveDateTime>,
    effects_off: bool,
    held_brightness_pct: u8,
    held_temperature_k: u32,
}

impl Default for RuntimeControls {
    fn default() -> Self {
        Self {
            automatic: true,
            brightness_offset_pct: 0,
            temperature_offset_k: 0,
            offset_expires_at: None,
            effects_off: false,
            held_brightness_pct: 100,
            held_temperature_k: 6500,
        }
    }
}

/// Live state snapshot exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct EngineState {
    pub phase: String,
    pub intensity: f64,
    pub scheduled_color_temp_k: u32,
    pub scheduled_brightness_pct: u8,
    pub color_temp_k: u32,
    pub brightness_pct: u8,
    pub sunrise: String,
    pub sunset: String,
    pub next_transition_label: String,
    pub next_transition_time: String,
    pub automatic: bool,
    pub effects_off: bool,
    pub app_bypassed: bool,
    pub brightness_offset_pct: i16,
    pub temperature_offset_k: i32,
    pub adjustment_expires_at: Option<String>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            phase: "day".into(),
            intensity: 0.0,
            scheduled_color_temp_k: 6500,
            scheduled_brightness_pct: 100,
            color_temp_k: 6500,
            brightness_pct: 100,
            sunrise: "--:--".into(),
            sunset: "--:--".into(),
            next_transition_label: "Calculating schedule".into(),
            next_transition_time: "--:--".into(),
            automatic: true,
            effects_off: false,
            app_bypassed: false,
            brightness_offset_pct: 0,
            temperature_offset_k: 0,
            adjustment_expires_at: None,
        }
    }
}

pub struct FadeEngine {
    state: Mutex<EngineState>,
    controls: Mutex<RuntimeControls>,
    stop: AtomicBool,
}

impl FadeEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(EngineState::default()),
            controls: Mutex::new(RuntimeControls::default()),
            stop: AtomicBool::new(false),
        })
    }

    pub fn get_state(&self) -> EngineState {
        self.state.lock().unwrap().clone()
    }

    pub fn set_automatic(&self, automatic: bool) {
        let current = self.get_state();
        let mut controls = self.controls.lock().unwrap();
        controls.automatic = automatic;
        controls.effects_off = false;
        if !automatic {
            controls.held_brightness_pct = current.brightness_pct;
            controls.held_temperature_k = current.color_temp_k;
        }
    }

    pub fn set_adjustments(&self, brightness_offset_pct: i16, temperature_offset_k: i32) {
        let settings = Settings::load();
        let now = Local::now().naive_local();
        let expiry = next_completed_boundary(&settings, now);
        let brightness_offset_pct = brightness_offset_pct.clamp(-100, 100);
        let temperature_offset_k = temperature_offset_k.clamp(-8200, 8200);
        {
            let mut controls = self.controls.lock().unwrap();
            controls.automatic = true;
            controls.effects_off = false;
            controls.brightness_offset_pct = brightness_offset_pct;
            controls.temperature_offset_k = temperature_offset_k;
            controls.offset_expires_at = if brightness_offset_pct == 0 && temperature_offset_k == 0
            {
                None
            } else {
                Some(expiry)
            };
        }

        // Apply immediately so direct manipulation is not gated by the one-second schedule tick.
        let current = self.get_state();
        let brightness =
            (current.scheduled_brightness_pct as i16 + brightness_offset_pct).clamp(0, 100) as u8;
        let temperature = (current.scheduled_color_temp_k as i32 + temperature_offset_k)
            .clamp(1800, 10000) as u32;
        apply_values(&settings, temperature, brightness);
        let mut state = self.state.lock().unwrap();
        state.automatic = true;
        state.effects_off = false;
        state.brightness_pct = brightness;
        state.color_temp_k = temperature;
        state.brightness_offset_pct = brightness_offset_pct;
        state.temperature_offset_k = temperature_offset_k;
        state.adjustment_expires_at = if brightness_offset_pct == 0 && temperature_offset_k == 0 {
            None
        } else {
            Some(clock(expiry))
        };
    }

    pub fn reset_adjustments(&self) {
        {
            let mut controls = self.controls.lock().unwrap();
            controls.brightness_offset_pct = 0;
            controls.temperature_offset_k = 0;
            controls.offset_expires_at = None;
        }
        let settings = Settings::load();
        let current = self.get_state();
        apply_values(
            &settings,
            current.scheduled_color_temp_k,
            current.scheduled_brightness_pct,
        );
        let mut state = self.state.lock().unwrap();
        state.color_temp_k = state.scheduled_color_temp_k;
        state.brightness_pct = state.scheduled_brightness_pct;
        state.brightness_offset_pct = 0;
        state.temperature_offset_k = 0;
        state.adjustment_expires_at = None;
    }

    pub fn set_effects_off(&self, effects_off: bool) {
        let mut controls = self.controls.lock().unwrap();
        controls.effects_off = effects_off;
        if effects_off {
            gamma::reset_gamma();
            let settings = Settings::load();
            ddcci::set_all_brightness(settings.brightness.day_percent);
        }
    }

    /// Compatibility for existing hotkeys: toggle automatic hold instead of resetting gamma.
    pub fn toggle_pause(&self) -> bool {
        let automatic = self.controls.lock().unwrap().automatic;
        self.set_automatic(!automatic);
        automatic
    }

    /// Compatibility for existing hotkeys: create temporary endpoint adjustments.
    pub fn jump_to(&self, night: bool) {
        let settings = Settings::load();
        let state = self.get_state();
        let target_brightness = if night {
            settings.brightness.night_percent
        } else {
            settings.brightness.day_percent
        };
        let target_temperature = if night {
            settings.color.night_temp_k
        } else {
            settings.color.day_temp_k
        };
        self.set_adjustments(
            target_brightness as i16 - state.scheduled_brightness_pct as i16,
            target_temperature as i32 - state.scheduled_color_temp_k as i32,
        );
    }

    pub fn start(self: &Arc<Self>) -> thread::JoinHandle<()> {
        let engine = Arc::clone(self);
        thread::spawn(move || {
            let mut cached_sun: Option<sun::SunTimes> = None;
            let mut last_sun_calc = None;
            let mut last_location: Option<(f64, f64)> = None;
            let mut tick_count = 0_u64;
            let mut last_phase = String::new();

            while !engine.stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(1));
                tick_count += 1;
                let settings = Settings::load();
                let now = Local::now().naive_local();
                let today = now.date();
                let location = (settings.location.latitude, settings.location.longitude);
                let location_changed = last_location
                    .map(|old| {
                        (old.0 - location.0).abs() > f64::EPSILON
                            || (old.1 - location.1).abs() > f64::EPSILON
                    })
                    .unwrap_or(true);

                if last_sun_calc != Some(today) || location_changed || tick_count % 3600 == 1 {
                    cached_sun = Some(sun::calculate_today(location.0, location.1));
                    last_sun_calc = Some(today);
                    last_location = Some(location);
                }
                let calculated_sun = match &cached_sun {
                    Some(value) => value,
                    None => continue,
                };
                let mut effective_sun = calculated_sun.clone();
                if settings.fade.use_civil_twilight {
                    effective_sun.sunrise = calculated_sun.civil_dawn;
                    effective_sun.sunset = calculated_sun.civil_dusk;
                }

                let (intensity, phase) = sun::current_intensity(
                    &effective_sun,
                    now,
                    settings.fade.fade_duration_min,
                    settings.fade.evening_offset_min,
                    settings.fade.morning_offset_min,
                );
                let scheduled_temperature = lerp_kelvin(
                    settings.color.day_temp_k,
                    settings.color.night_temp_k,
                    intensity,
                );
                let scheduled_brightness = lerp_brightness(&settings, intensity);
                let app_bypassed = appdetect::should_pause(&settings.pause_apps);

                let (
                    automatic,
                    effects_off,
                    brightness_offset,
                    temperature_offset,
                    expiry,
                    held_brightness,
                    held_temperature,
                ) = {
                    let mut controls = engine.controls.lock().unwrap();
                    if controls.offset_expires_at.is_some_and(|value| now >= value) {
                        controls.brightness_offset_pct = 0;
                        controls.temperature_offset_k = 0;
                        controls.offset_expires_at = None;
                    }
                    (
                        controls.automatic,
                        controls.effects_off,
                        controls.brightness_offset_pct,
                        controls.temperature_offset_k,
                        controls.offset_expires_at,
                        controls.held_brightness_pct,
                        controls.held_temperature_k,
                    )
                };

                let (effective_brightness, effective_temperature) = if effects_off || app_bypassed {
                    gamma::reset_gamma();
                    ddcci::set_all_brightness(settings.brightness.day_percent);
                    (settings.brightness.day_percent, 6500)
                } else if !automatic {
                    apply_values(&settings, held_temperature, held_brightness);
                    (held_brightness, held_temperature)
                } else {
                    let brightness =
                        (scheduled_brightness as i16 + brightness_offset).clamp(0, 100) as u8;
                    let temperature = (scheduled_temperature as i32 + temperature_offset)
                        .clamp(1800, 10000) as u32;
                    apply_values(&settings, temperature, brightness);
                    (brightness, temperature)
                };

                if automatic
                    && !effects_off
                    && !app_bypassed
                    && settings.theme.auto_switch
                    && phase != last_phase
                {
                    let is_night = phase == "night";
                    let was_night = last_phase == "night";
                    if is_night && !was_night {
                        theme::set_dark_theme(settings.theme.dark_at_night);
                    }
                    if !is_night && was_night {
                        theme::set_dark_theme(!settings.theme.dark_at_night);
                    }
                }
                last_phase = phase.to_string();

                let (next_label, next_time) = next_transition(&settings, now);
                let mut state = engine.state.lock().unwrap();
                state.phase = phase.to_string();
                state.intensity = intensity;
                state.scheduled_color_temp_k = scheduled_temperature;
                state.scheduled_brightness_pct = scheduled_brightness;
                state.color_temp_k = effective_temperature;
                state.brightness_pct = effective_brightness;
                state.sunrise = clock(calculated_sun.sunrise);
                state.sunset = clock(calculated_sun.sunset);
                state.next_transition_label = next_label;
                state.next_transition_time = next_time;
                state.automatic = automatic;
                state.effects_off = effects_off;
                state.app_bypassed = app_bypassed;
                state.brightness_offset_pct = brightness_offset;
                state.temperature_offset_k = temperature_offset;
                state.adjustment_expires_at = expiry.map(clock);
            }
            gamma::reset_gamma();
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn apply_values(settings: &Settings, color_temp: u32, brightness_pct: u8) -> bool {
    let rgb = kelvin_to_rgb(color_temp);
    ddcci::set_all_brightness(brightness_pct);
    let ramps = GammaRamps::from_color_and_brightness(&rgb, brightness_pct as f64 / 100.0);
    match settings.engine {
        crate::config::EngineKind::GammaRamps | crate::config::EngineKind::NightLight => {
            gamma::set_gamma_ramp(&ramps)
        }
    }
}

fn lerp_brightness(settings: &Settings, intensity: f64) -> u8 {
    let day = settings.brightness.day_percent as f64;
    let night = settings.brightness.night_percent as f64;
    (day + (night - day) * intensity).round().clamp(0.0, 100.0) as u8
}

fn effective_sun(settings: &Settings, date: chrono::NaiveDate) -> sun::SunTimes {
    let offset = Local::now().offset().local_minus_utc() as f64 / 3600.0;
    let mut value = sun::calculate_sun_times(
        date,
        settings.location.latitude,
        settings.location.longitude,
        offset,
    );
    if settings.fade.use_civil_twilight {
        value.sunrise = value.civil_dawn;
        value.sunset = value.civil_dusk;
    }
    value
}

fn boundaries(settings: &Settings, date: chrono::NaiveDate) -> (NaiveDateTime, NaiveDateTime) {
    let solar = effective_sun(settings, date);
    let morning_end =
        solar.sunrise + ChronoDuration::minutes(settings.fade.morning_offset_min as i64);
    let evening_end =
        solar.sunset - ChronoDuration::minutes(settings.fade.evening_offset_min as i64);
    (morning_end, evening_end)
}

fn next_completed_boundary(settings: &Settings, now: NaiveDateTime) -> NaiveDateTime {
    let (morning, evening) = boundaries(settings, now.date());
    if now < morning {
        morning
    } else if now < evening {
        evening
    } else {
        boundaries(settings, now.date() + ChronoDuration::days(1)).0
    }
}

fn next_transition(settings: &Settings, now: NaiveDateTime) -> (String, String) {
    let (morning, evening) = boundaries(settings, now.date());
    if now < morning {
        ("Day mode".into(), clock(morning))
    } else if now < evening {
        ("Night mode".into(), clock(evening))
    } else {
        let tomorrow = boundaries(settings, now.date() + ChronoDuration::days(1)).0;
        ("Day mode".into(), clock(tomorrow))
    }
}

fn clock(value: NaiveDateTime) -> String {
    format!("{:02}:{:02}", value.time().hour(), value.time().minute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    #[test]
    fn brightness_is_clamped() {
        let mut settings = Settings::default();
        settings.brightness.day_percent = 100;
        settings.brightness.night_percent = 0;
        assert_eq!(lerp_brightness(&settings, -1.0), 100);
        assert_eq!(lerp_brightness(&settings, 2.0), 0);
    }

    #[test]
    fn next_boundary_is_in_the_future() {
        let settings = Settings::default();
        let now = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 7, 19).unwrap(),
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
        );
        assert!(next_completed_boundary(&settings, now) > now);
    }
}
