//! Solar schedule engine, independent display controls, and schedule preview.

use crate::appdetect;
use crate::color::{kelvin_to_rgb, lerp_kelvin};
use crate::config::Settings;
use crate::ddcci;
use crate::gamma::{self, GammaRamps};
use crate::{sun, theme};
use chrono::{Duration as ChronoDuration, Local, NaiveDateTime, NaiveTime, Timelike};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
struct RuntimeControls {
    automatic: bool,
    hardware_offset_pct: i16,
    overlay_offset_pct: i16,
    temperature_offset_k: i32,
    offset_expires_at: Option<NaiveDateTime>,
    effects_off: bool,
    held_hardware_pct: u8,
    held_overlay_pct: u8,
    held_temperature_k: u32,
    preview_minute: Option<u16>,
}

impl Default for RuntimeControls {
    fn default() -> Self {
        Self {
            automatic: true,
            hardware_offset_pct: 0,
            overlay_offset_pct: 0,
            temperature_offset_k: 0,
            offset_expires_at: None,
            effects_off: false,
            held_hardware_pct: 100,
            held_overlay_pct: 100,
            held_temperature_k: 6500,
            preview_minute: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ScheduleValues {
    intensity: f64,
    phase: String,
    hardware_pct: u8,
    overlay_pct: u8,
    temperature_k: u32,
}

/// Live state snapshot exposed to both frontends.
#[derive(Debug, Clone, Serialize)]
pub struct EngineState {
    pub phase: String,
    pub intensity: f64,
    pub scheduled_color_temp_k: u32,
    pub scheduled_hardware_brightness_pct: u8,
    pub scheduled_overlay_brightness_pct: u8,
    pub color_temp_k: u32,
    pub hardware_brightness_pct: u8,
    pub overlay_brightness_pct: u8,
    pub sunrise: String,
    pub sunset: String,
    pub next_transition_label: String,
    pub next_transition_time: String,
    pub automatic: bool,
    pub effects_off: bool,
    pub app_bypassed: bool,
    pub hardware_offset_pct: i16,
    pub overlay_offset_pct: i16,
    pub temperature_offset_k: i32,
    pub adjustment_expires_at: Option<String>,
    pub preview_minute: Option<u16>,
    pub preview_theme_dark: Option<bool>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            phase: "day".into(),
            intensity: 0.0,
            scheduled_color_temp_k: 6500,
            scheduled_hardware_brightness_pct: 100,
            scheduled_overlay_brightness_pct: 100,
            color_temp_k: 6500,
            hardware_brightness_pct: 100,
            overlay_brightness_pct: 100,
            sunrise: "--:--".into(),
            sunset: "--:--".into(),
            next_transition_label: "Calculating schedule".into(),
            next_transition_time: "--:--".into(),
            automatic: true,
            effects_off: false,
            app_bypassed: false,
            hardware_offset_pct: 0,
            overlay_offset_pct: 0,
            temperature_offset_k: 0,
            adjustment_expires_at: None,
            preview_minute: None,
            preview_theme_dark: None,
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
            controls.held_hardware_pct = current.hardware_brightness_pct;
            controls.held_overlay_pct = current.overlay_brightness_pct;
            controls.held_temperature_k = current.color_temp_k;
        }
    }

    pub fn set_adjustments(
        &self,
        hardware_offset_pct: i16,
        overlay_offset_pct: i16,
        temperature_offset_k: i32,
    ) {
        let settings = Settings::load();
        let now = Local::now().naive_local();
        let expiry = next_completed_boundary(&settings, now);
        let hardware_offset_pct = hardware_offset_pct.clamp(-100, 100);
        let overlay_offset_pct = overlay_offset_pct.clamp(-95, 95);
        let temperature_offset_k = temperature_offset_k.clamp(-8200, 8200);
        {
            let mut controls = self.controls.lock().unwrap();
            controls.automatic = true;
            controls.effects_off = false;
            controls.hardware_offset_pct = hardware_offset_pct;
            controls.overlay_offset_pct = overlay_offset_pct;
            controls.temperature_offset_k = temperature_offset_k;
            controls.offset_expires_at =
                if hardware_offset_pct == 0 && overlay_offset_pct == 0 && temperature_offset_k == 0
                {
                    None
                } else {
                    Some(expiry)
                };
        }

        let current = self.get_state();
        let hardware = offset_percent(
            current.scheduled_hardware_brightness_pct,
            hardware_offset_pct,
            0,
        );
        let overlay = offset_percent(
            current.scheduled_overlay_brightness_pct,
            overlay_offset_pct,
            5,
        );
        let temperature = (current.scheduled_color_temp_k as i32 + temperature_offset_k)
            .clamp(1800, 10000) as u32;
        apply_values(temperature, hardware, overlay);
        let mut state = self.state.lock().unwrap();
        state.automatic = true;
        state.effects_off = false;
        state.hardware_brightness_pct = hardware;
        state.overlay_brightness_pct = overlay;
        state.color_temp_k = temperature;
        state.hardware_offset_pct = hardware_offset_pct;
        state.overlay_offset_pct = overlay_offset_pct;
        state.temperature_offset_k = temperature_offset_k;
        state.adjustment_expires_at = if hardware_offset_pct == 0
            && overlay_offset_pct == 0
            && temperature_offset_k == 0
        {
            None
        } else {
            Some(clock(expiry))
        };
    }

    pub fn reset_adjustments(&self) {
        {
            let mut controls = self.controls.lock().unwrap();
            controls.hardware_offset_pct = 0;
            controls.overlay_offset_pct = 0;
            controls.temperature_offset_k = 0;
            controls.offset_expires_at = None;
        }
        self.restore_current_time();
    }

    pub fn set_effects_off(&self, effects_off: bool) {
        self.controls.lock().unwrap().effects_off = effects_off;
        if effects_off {
            let settings = Settings::load();
            gamma::reset_gamma();
            ddcci::set_all_brightness(settings.brightness.hardware_day_percent);
        } else {
            self.restore_current_time();
        }
    }

    pub fn set_preview_minute(&self, minute: Option<u16>) {
        self.controls.lock().unwrap().preview_minute = minute.map(|value| value.min(1439));
        if let Some(minute) = minute {
            let settings = Settings::load();
            let now = at_minute(Local::now().naive_local(), minute.min(1439));
            let values = schedule_values(&settings, now);
            apply_values(
                values.temperature_k,
                values.hardware_pct,
                values.overlay_pct,
            );
            let mut state = self.state.lock().unwrap();
            state.preview_minute = Some(minute.min(1439));
            state.preview_theme_dark = Some(theme_should_be_dark(&settings, now));
            state.hardware_brightness_pct = values.hardware_pct;
            state.overlay_brightness_pct = values.overlay_pct;
            state.color_temp_k = values.temperature_k;
        } else {
            self.restore_current_time();
        }
    }

    fn restore_current_time(&self) {
        let settings = Settings::load();
        let now = Local::now().naive_local();
        let scheduled = schedule_values(&settings, now);
        let controls = self.controls.lock().unwrap().clone();
        let (hardware, overlay, temperature) = effective_values(&scheduled, &controls);
        apply_values(temperature, hardware, overlay);
        let mut state = self.state.lock().unwrap();
        state.preview_minute = None;
        state.preview_theme_dark = None;
        state.hardware_brightness_pct = hardware;
        state.overlay_brightness_pct = overlay;
        state.color_temp_k = temperature;
    }

    pub fn toggle_pause(&self) -> bool {
        let automatic = self.controls.lock().unwrap().automatic;
        self.set_automatic(!automatic);
        automatic
    }

    pub fn jump_to(&self, night: bool) {
        let settings = Settings::load();
        let state = self.get_state();
        let hardware = if night {
            settings.brightness.hardware_night_percent
        } else {
            settings.brightness.hardware_day_percent
        };
        let overlay = if night {
            settings.brightness.overlay_night_percent
        } else {
            settings.brightness.overlay_day_percent
        };
        let temperature = if night {
            settings.color.night_temp_k
        } else {
            settings.color.day_temp_k
        };
        self.set_adjustments(
            hardware as i16 - state.scheduled_hardware_brightness_pct as i16,
            overlay as i16 - state.scheduled_overlay_brightness_pct as i16,
            temperature as i32 - state.scheduled_color_temp_k as i32,
        );
    }

    pub fn step_brightness(&self, delta_pct: i16) {
        let state = self.get_state();
        let target = stepped_brightness(state.hardware_brightness_pct, delta_pct) as i16;
        self.set_adjustments(
            target - state.scheduled_hardware_brightness_pct as i16,
            state.overlay_brightness_pct as i16 - state.scheduled_overlay_brightness_pct as i16,
            state.color_temp_k as i32 - state.scheduled_color_temp_k as i32,
        );
    }

    pub fn start(self: &Arc<Self>) -> thread::JoinHandle<()> {
        let engine = Arc::clone(self);
        thread::spawn(move || {
            let mut last_theme_dark = None;
            while !engine.stop.load(Ordering::Relaxed) {
                let settings = Settings::load();
                let real_now = Local::now().naive_local();
                let mut controls = engine.controls.lock().unwrap().clone();
                if controls
                    .offset_expires_at
                    .is_some_and(|value| real_now >= value)
                {
                    let mut live = engine.controls.lock().unwrap();
                    live.hardware_offset_pct = 0;
                    live.overlay_offset_pct = 0;
                    live.temperature_offset_k = 0;
                    live.offset_expires_at = None;
                    controls = live.clone();
                }
                let render_now = controls
                    .preview_minute
                    .map(|minute| at_minute(real_now, minute))
                    .unwrap_or(real_now);
                let scheduled = schedule_values(&settings, render_now);
                let app_bypassed = appdetect::should_pause(&settings.pause_apps);
                let (hardware, overlay, temperature) = if controls.preview_minute.is_some() {
                    (
                        scheduled.hardware_pct,
                        scheduled.overlay_pct,
                        scheduled.temperature_k,
                    )
                } else if controls.effects_off || app_bypassed {
                    gamma::reset_gamma();
                    ddcci::set_all_brightness(settings.brightness.hardware_day_percent);
                    (settings.brightness.hardware_day_percent, 100, 6500)
                } else {
                    effective_values(&scheduled, &controls)
                };
                if !controls.effects_off && !app_bypassed {
                    apply_values(temperature, hardware, overlay);
                }

                if settings.theme.auto_switch && controls.preview_minute.is_none() {
                    let desired_dark = theme_should_be_dark(&settings, real_now);
                    if last_theme_dark != Some(desired_dark) {
                        theme::set_dark_theme(desired_dark);
                        last_theme_dark = Some(desired_dark);
                    }
                } else {
                    last_theme_dark = None;
                }

                let solar = effective_sun(&settings, real_now.date());
                let (next_label, next_time) = next_transition(&settings, real_now);
                let mut state = engine.state.lock().unwrap();
                state.phase = scheduled.phase;
                state.intensity = scheduled.intensity;
                state.scheduled_color_temp_k = scheduled.temperature_k;
                state.scheduled_hardware_brightness_pct = scheduled.hardware_pct;
                state.scheduled_overlay_brightness_pct = scheduled.overlay_pct;
                state.color_temp_k = temperature;
                state.hardware_brightness_pct = hardware;
                state.overlay_brightness_pct = overlay;
                state.sunrise = clock(solar.sunrise);
                state.sunset = clock(solar.sunset);
                state.next_transition_label = next_label;
                state.next_transition_time = next_time;
                state.automatic = controls.automatic;
                state.effects_off = controls.effects_off;
                state.app_bypassed = app_bypassed;
                state.hardware_offset_pct = controls.hardware_offset_pct;
                state.overlay_offset_pct = controls.overlay_offset_pct;
                state.temperature_offset_k = controls.temperature_offset_k;
                state.adjustment_expires_at = controls.offset_expires_at.map(clock);
                state.preview_minute = controls.preview_minute;
                state.preview_theme_dark = controls
                    .preview_minute
                    .map(|_| theme_should_be_dark(&settings, render_now));
                drop(state);
                thread::sleep(Duration::from_secs(1));
            }
            gamma::reset_gamma();
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn effective_values(scheduled: &ScheduleValues, controls: &RuntimeControls) -> (u8, u8, u32) {
    if !controls.automatic {
        return (
            controls.held_hardware_pct,
            controls.held_overlay_pct,
            controls.held_temperature_k,
        );
    }
    (
        offset_percent(scheduled.hardware_pct, controls.hardware_offset_pct, 0),
        offset_percent(scheduled.overlay_pct, controls.overlay_offset_pct, 5),
        (scheduled.temperature_k as i32 + controls.temperature_offset_k).clamp(1800, 10000) as u32,
    )
}

fn apply_values(color_temp: u32, hardware_pct: u8, overlay_pct: u8) -> bool {
    let rgb = kelvin_to_rgb(color_temp);
    ddcci::set_all_brightness(hardware_pct);
    let ramps = GammaRamps::from_color_and_brightness(&rgb, overlay_pct as f64 / 100.0);
    gamma::set_gamma_ramp(&ramps)
}

fn schedule_values(settings: &Settings, now: NaiveDateTime) -> ScheduleValues {
    let solar = effective_sun(settings, now.date());
    let (intensity, phase) = sun::current_intensity(
        &solar,
        now,
        settings.fade.fade_duration_min,
        settings.fade.evening_offset_min,
        settings.fade.morning_offset_min,
    );
    ScheduleValues {
        intensity,
        phase: phase.into(),
        hardware_pct: lerp_percent(
            settings.brightness.hardware_day_percent,
            settings.brightness.hardware_night_percent,
            intensity,
            0,
        ),
        overlay_pct: lerp_percent(
            settings.brightness.overlay_day_percent,
            settings.brightness.overlay_night_percent,
            intensity,
            5,
        ),
        temperature_k: lerp_kelvin(
            settings.color.day_temp_k,
            settings.color.night_temp_k,
            intensity,
        ),
    }
}

fn lerp_percent(day: u8, night: u8, intensity: f64, floor: u8) -> u8 {
    (day as f64 + (night as f64 - day as f64) * intensity)
        .round()
        .clamp(floor as f64, 100.0) as u8
}

fn offset_percent(value: u8, offset: i16, floor: u8) -> u8 {
    (value as i16 + offset).clamp(floor as i16, 100) as u8
}

fn stepped_brightness(current: u8, delta_pct: i16) -> u8 {
    (current as i16 + delta_pct).clamp(0, 100) as u8
}

fn at_minute(now: NaiveDateTime, minute: u16) -> NaiveDateTime {
    NaiveDateTime::new(
        now.date(),
        NaiveTime::from_hms_opt((minute / 60) as u32, (minute % 60) as u32, 0).unwrap(),
    )
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

fn theme_boundaries(
    settings: &Settings,
    date: chrono::NaiveDate,
) -> (NaiveDateTime, NaiveDateTime) {
    let solar = effective_sun(settings, date);
    (
        solar.sunrise + ChronoDuration::minutes(settings.theme.light_offset_min as i64),
        solar.sunset + ChronoDuration::minutes(settings.theme.dark_offset_min as i64),
    )
}

fn theme_should_be_dark(settings: &Settings, now: NaiveDateTime) -> bool {
    let (light, dark) = theme_boundaries(settings, now.date());
    if light <= dark {
        now < light || now >= dark
    } else {
        now >= dark && now < light
    }
}

fn boundaries(settings: &Settings, date: chrono::NaiveDate) -> (NaiveDateTime, NaiveDateTime) {
    let solar = effective_sun(settings, date);
    (
        solar.sunrise + ChronoDuration::minutes(settings.fade.morning_offset_min as i64),
        solar.sunset - ChronoDuration::minutes(settings.fade.evening_offset_min as i64),
    )
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
    fn independent_percent_curves_use_their_own_floor() {
        assert_eq!(lerp_percent(100, 0, 1.0, 0), 0);
        assert_eq!(lerp_percent(100, 0, 1.0, 5), 5);
        assert_eq!(lerp_percent(100, 50, 0.5, 0), 75);
    }

    #[test]
    fn brightness_step_target_is_clamped() {
        assert_eq!(stepped_brightness(98, 5), 100);
        assert_eq!(stepped_brightness(2, -5), 0);
    }

    #[test]
    fn theme_interval_crosses_midnight() {
        let settings = Settings::default();
        let date = NaiveDate::from_ymd_opt(2026, 7, 19).unwrap();
        let (light, dark) = theme_boundaries(&settings, date);
        assert!(!theme_should_be_dark(
            &settings,
            light + ChronoDuration::minutes(1)
        ));
        assert!(theme_should_be_dark(
            &settings,
            dark + ChronoDuration::minutes(1)
        ));
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
