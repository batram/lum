//! Solar position calculations using the NOAA algorithm.
//! Computes sunrise/sunset times from latitude/longitude without network access.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};

/// Result of a sun calculation for a given date.
#[derive(Debug, Clone)]
pub struct SunTimes {
    /// Sunrise time (local).
    pub sunrise: NaiveDateTime,
    /// Sunset time (local).
    pub sunset: NaiveDateTime,
    /// Civil twilight start (evening, sun at -6°).
    pub civil_dusk: NaiveDateTime,
    /// Civil twilight end (morning, sun at -6°).
    pub civil_dawn: NaiveDateTime,
}

const DEG_TO_RAD: f64 = std::f64::consts::PI / 180.0;
const RAD_TO_DEG: f64 = 180.0 / std::f64::consts::PI;

/// Calculate sunrise, sunset, and civil twilight for a given date and location.
///
/// Uses the NOAA solar position algorithm.
/// `utc_offset_hours` is the local timezone offset from UTC (e.g. -5 for EST).
pub fn calculate_sun_times(
    date: NaiveDate,
    latitude: f64,
    longitude: f64,
    utc_offset_hours: f64,
) -> SunTimes {
    let sunrise = calc_sun_event(date, latitude, longitude, utc_offset_hours, true, 0.833);
    let sunset = calc_sun_event(date, latitude, longitude, utc_offset_hours, false, 0.833);
    let civil_dawn = calc_sun_event(date, latitude, longitude, utc_offset_hours, true, 6.0);
    let civil_dusk = calc_sun_event(date, latitude, longitude, utc_offset_hours, false, 6.0);

    SunTimes {
        sunrise,
        sunset,
        civil_dusk,
        civil_dawn,
    }
}

/// Calculate sun times for today using the system's local timezone offset.
pub fn calculate_today(latitude: f64, longitude: f64) -> SunTimes {
    let now = Local::now();
    let date = now.date_naive();
    let offset = now.offset().local_minus_utc() as f64 / 3600.0;
    calculate_sun_times(date, latitude, longitude, offset)
}

/// Core NOAA calculation for a sun event (rise or set) at a given zenith angle.
///
/// `zenith_deg`: 0.833 for official sunrise/sunset, 6.0 for civil twilight, etc.
/// `is_sunrise`: true for rising event, false for setting event.
fn calc_sun_event(
    date: NaiveDate,
    latitude: f64,
    longitude: f64,
    utc_offset: f64,
    is_sunrise: bool,
    zenith_deg: f64,
) -> NaiveDateTime {
    // Day of year
    let day_of_year = date.ordinal() as f64;

    // Approximate time
    let lng_hour = longitude / 15.0;
    let t = if is_sunrise {
        day_of_year + ((6.0 - lng_hour) / 24.0)
    } else {
        day_of_year + ((18.0 - lng_hour) / 24.0)
    };

    // Sun's mean anomaly
    let m = (0.9856 * t) - 3.289;

    // Sun's true longitude
    let mut l = m + (1.916 * sin_deg(m)) + (0.020 * sin_deg(2.0 * m)) + 282.634;
    l = normalize_deg(l);

    // Sun's right ascension
    let mut ra = RAD_TO_DEG * (0.91764 * tan_deg(l)).atan();
    ra = normalize_deg(ra);

    // Right ascension needs to be in the same quadrant as L
    let l_quadrant = (l / 90.0).floor() * 90.0;
    let ra_quadrant = (ra / 90.0).floor() * 90.0;
    ra += l_quadrant - ra_quadrant;

    // Convert to hours
    ra /= 15.0;

    // Sun's declination
    let sin_dec = 0.39782 * sin_deg(l);
    let cos_dec = cos_from_sin(sin_dec);

    // Sun's local hour angle
    let cos_h = (cos_deg(zenith_deg) - (sin_dec * sin_deg(latitude)))
        / (cos_dec * cos_deg(latitude));

    // Clamp for polar regions (sun never rises/sets)
    let cos_h = cos_h.clamp(-1.0, 1.0);

    // Calculate H
    let h = if is_sunrise {
        360.0 - (RAD_TO_DEG * cos_h.acos())
    } else {
        RAD_TO_DEG * cos_h.acos()
    };
    let h = h / 15.0; // Convert to hours

    // Local mean time of event
    let local_t = h + ra - (0.06571 * t) - 6.622;

    // Adjust to UTC
    let mut utc = local_t - lng_hour;
    utc = normalize_hours(utc);

    // Convert to local time
    let mut local_hours = utc + utc_offset;
    local_hours = normalize_hours(local_hours);

    // Convert fractional hours to NaiveDateTime
    let total_seconds = (local_hours * 3600.0).round() as i64;
    let hours = (total_seconds / 3600) as u32;
    let minutes = ((total_seconds % 3600) / 60) as u32;
    let seconds = (total_seconds % 60) as u32;

    let time = NaiveTime::from_hms_opt(hours, minutes, seconds)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    NaiveDateTime::new(date, time)
}

/// Determine the current phase and transition progress (0.0 = full day, 1.0 = full night).
///
/// Returns `(intensity, phase_name)` where intensity drives the color/brightness curves.
pub fn current_intensity(
    sun: &SunTimes,
    now: NaiveDateTime,
    fade_duration_min: u32,
    evening_offset_min: i32,
    morning_offset_min: i32,
) -> (f64, &'static str) {
    let fade_dur = Duration::minutes(fade_duration_min as i64);
    let evening_offset = Duration::minutes(evening_offset_min as i64);
    let morning_offset = Duration::minutes(morning_offset_min as i64);

    // Evening fade: starts at (sunset - offset - fade_duration), ends at (sunset - offset)
    let evening_end = sun.sunset - evening_offset;
    let evening_start = evening_end - fade_dur;

    // Morning fade: starts at (sunrise + offset - fade_duration), ends at (sunrise + offset)
    // During morning, intensity goes from 1 (night) → 0 (day)
    let morning_end = sun.sunrise + morning_offset;
    let morning_start = morning_end - fade_dur;

    if now >= evening_end || now < morning_start {
        // Full night
        (1.0, "night")
    } else if now >= evening_start && now < evening_end {
        // Evening fade: 0 → 1
        let elapsed = (now - evening_start).num_seconds() as f64;
        let total = fade_dur.num_seconds() as f64;
        let progress = (elapsed / total).clamp(0.0, 1.0);
        (progress, "evening")
    } else if now >= morning_start && now < morning_end {
        // Morning fade: 1 → 0
        let elapsed = (now - morning_start).num_seconds() as f64;
        let total = fade_dur.num_seconds() as f64;
        let progress = (elapsed / total).clamp(0.0, 1.0);
        (1.0 - progress, "morning")
    } else {
        // Full day (between morning_end and evening_start)
        (0.0, "day")
    }
}

// --- Trig helpers (input in degrees) ---

fn sin_deg(deg: f64) -> f64 {
    (deg * DEG_TO_RAD).sin()
}

fn cos_deg(deg: f64) -> f64 {
    (deg * DEG_TO_RAD).cos()
}

fn tan_deg(deg: f64) -> f64 {
    (deg * DEG_TO_RAD).tan()
}

fn cos_from_sin(sin_val: f64) -> f64 {
    (1.0 - sin_val * sin_val).sqrt()
}

fn normalize_deg(mut deg: f64) -> f64 {
    while deg < 0.0 {
        deg += 360.0;
    }
    while deg >= 360.0 {
        deg -= 360.0;
    }
    deg
}

fn normalize_hours(mut h: f64) -> f64 {
    while h < 0.0 {
        h += 24.0;
    }
    while h >= 24.0 {
        h -= 24.0;
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_nyc_summer_sunrise_sunset() {
        // June 21 in NYC: sunrise ~5:25, sunset ~20:31 (EDT, UTC-4)
        let date = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let sun = calculate_sun_times(date, 40.7128, -74.0060, -4.0);

        // Sunrise should be around 5:25 (allow ±15 min tolerance)
        assert!(sun.sunrise.time().hour() >= 5 && sun.sunrise.time().hour() <= 6);
        // Sunset should be around 20:30
        assert!(sun.sunset.time().hour() >= 20 && sun.sunset.time().hour() <= 21);
    }

    #[test]
    fn test_intensity_phases() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let sun = calculate_sun_times(date, 40.7128, -74.0060, -4.0);

        // Midday should be 0 (full day)
        let midday = NaiveDateTime::new(date, NaiveTime::from_hms_opt(12, 0, 0).unwrap());
        let (intensity, phase) = current_intensity(&sun, midday, 60, 0, 0);
        assert_eq!(phase, "day");
        assert!((intensity - 0.0).abs() < 0.01);

        // Midnight should be 1 (full night)
        let midnight = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let (intensity, phase) = current_intensity(&sun, midnight, 60, 0, 0);
        assert_eq!(phase, "night");
        assert!((intensity - 1.0).abs() < 0.01);
    }
}
